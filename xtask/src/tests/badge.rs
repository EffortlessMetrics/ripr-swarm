use super::*;

#[test]
fn badge_artifacts_summary_markdown_includes_counts_and_warnings() -> Result<(), String> {
    let markdown =
        badge_artifacts_summary_markdown(STUB_RIPR_NATIVE_JSON, STUB_RIPR_PLUS_NATIVE_JSON);

    let expectations = [
        "## ripr",
        "## ripr+",
        "- message: 3",
        "- message: 7",
        "- color: yellow",
        "- color: orange",
        "  - unsuppressed_exposure_gaps: 3",
        "  - unsuppressed_test_efficiency_findings: 7",
        "  - broad_oracle: 4",
        "  - first warning",
        "  - second warning",
    ];
    for expected in expectations {
        if !markdown.contains(expected) {
            return Err(format!(
                "expected '{expected}' in markdown, got:\n{markdown}"
            ));
        }
    }
    Ok(())
}

#[test]
fn badge_artifacts_summary_markdown_omits_forbidden_terms() -> Result<(), String> {
    let markdown =
        badge_artifacts_summary_markdown(STUB_RIPR_NATIVE_JSON, STUB_RIPR_PLUS_NATIVE_JSON);

    let forbidden = ["/", "coverage", "uncovered", "killed", "proven", "adequate"];
    let lower = markdown.to_lowercase();
    for term in forbidden {
        if lower.contains(&term.to_lowercase()) {
            return Err(format!(
                "forbidden term '{term}' found in markdown:\n{markdown}"
            ));
        }
    }
    // The "survived" check is intentionally substring-strict: the badge
    // schema's `unsuppressed_*` keys legitimately contain neither
    // "killed" nor "survived", but a future schema change could add
    // mutation-runtime language. Guard with a word-boundary heuristic
    // by requiring the leading character to be alphabetic-or-start.
    for forbidden_word in ["survived", "untested"] {
        for window in lower.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if window == forbidden_word {
                return Err(format!(
                    "forbidden word '{forbidden_word}' found in markdown:\n{markdown}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn badge_artifacts_summary_markdown_is_deterministic_with_sorted_keys() -> Result<(), String> {
    let unsorted_ripr = r#"{
  "label": "ripr",
  "message": "0",
  "color": "brightgreen",
  "counts": {
    "unsuppressed_test_efficiency_findings": 0,
    "analyzed_tests": 0,
    "unsuppressed_exposure_gaps": 0,
    "intentional_test_efficiency_findings": 0,
    "suppressed_exposure_gaps": 0,
    "unknowns": 0,
    "analyzed_findings": 0,
    "suppressed_test_efficiency_findings": 0,
    "unknowns_test_efficiency": 0
  },
  "reason_counts": {
    "smoke_oracle_only": 0,
    "no_assertion_detected": 0,
    "broad_oracle": 0,
    "relational_oracle": 0
  },
  "warnings": []
}"#;
    let unsorted_ripr_plus = r#"{
  "label": "ripr+",
  "message": "0",
  "color": "brightgreen",
  "counts": {
    "analyzed_tests": 308,
    "unsuppressed_test_efficiency_findings": 0,
    "unknowns_test_efficiency": 0,
    "analyzed_findings": 0,
    "suppressed_test_efficiency_findings": 0,
    "unsuppressed_exposure_gaps": 0,
    "unknowns": 0,
    "intentional_test_efficiency_findings": 0,
    "suppressed_exposure_gaps": 0
  },
  "reason_counts": {
    "smoke_oracle_only": 0,
    "broad_oracle": 0,
    "no_assertion_detected": 0,
    "relational_oracle": 0
  },
  "warnings": []
}"#;

    let markdown_a = badge_artifacts_summary_markdown(unsorted_ripr, unsorted_ripr_plus);
    let markdown_b = badge_artifacts_summary_markdown(unsorted_ripr, unsorted_ripr_plus);
    if markdown_a != markdown_b {
        return Err(format!(
            "markdown not deterministic across calls:\nA:\n{markdown_a}\nB:\n{markdown_b}"
        ));
    }

    // Within each `counts:` and `reason_counts:` block, bullet lines
    // (those starting with "  - ") must appear in lexicographic order.
    let mut in_block = false;
    let mut prev: Option<String> = None;
    for line in markdown_a.lines() {
        if line.starts_with("- counts:") || line.starts_with("- reason_counts:") {
            in_block = true;
            prev = None;
            continue;
        }
        if line.starts_with("- ") {
            in_block = false;
            prev = None;
            continue;
        }
        if in_block && line.starts_with("  - ") {
            let curr = line.to_string();
            if let Some(prior) = &prev
                && prior > &curr
            {
                return Err(format!(
                    "block keys not lexicographically sorted:\n{prior}\n{curr}"
                ));
            }
            prev = Some(curr);
        }
    }
    Ok(())
}

#[test]
fn badge_artifact_jobs_has_exactly_four_entries_in_documented_order() -> Result<(), String> {
    let jobs = badge_artifact_jobs();
    let expected = vec![
        BadgeArtifactJob {
            format: "badge-json",
            output_file: "ripr-badge.json",
        },
        BadgeArtifactJob {
            format: "badge-shields",
            output_file: "ripr-badge-shields.json",
        },
        BadgeArtifactJob {
            format: "badge-plus-json",
            output_file: "ripr-plus-badge.json",
        },
        BadgeArtifactJob {
            format: "badge-plus-shields",
            output_file: "ripr-plus-badge-shields.json",
        },
    ];
    if jobs == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {jobs:?}"))
    }
}

#[test]
fn badge_artifact_command_args_matches_documented_invocation() -> Result<(), String> {
    let args = badge_artifact_command_args("badge-plus-json");
    let expected: Vec<String> = [
        "check",
        "--root",
        ".",
        "--diff",
        "target/ripr/badge-input.diff",
        "--format",
        "badge-plus-json",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if args == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {args:?}"))
    }
}

#[test]
fn badge_artifact_command_args_substitutes_format_only() -> Result<(), String> {
    for format in ["badge-json", "badge-shields", "badge-plus-shields"] {
        let args = badge_artifact_command_args(format);
        let last = args.last().cloned().unwrap_or_default();
        if last != format {
            return Err(format!(
                "expected last arg to be {format:?}, got {last:?} (full args: {args:?})"
            ));
        }
        // The static prefix must be byte-identical across formats.
        let prefix = &args[..args.len() - 1];
        let expected_prefix: Vec<String> = [
            "check",
            "--root",
            ".",
            "--diff",
            "target/ripr/badge-input.diff",
            "--format",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if prefix != expected_prefix.as_slice() {
            return Err(format!(
                "static arg prefix changed for {format:?}: expected {expected_prefix:?}, got {prefix:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn badge_diff_reader_resolves_a_non_main_default_base_and_diffs_it() -> Result<(), String> {
    with_temp_cwd(
        "badge-diff-non-main-default",
        |root| -> Result<(), String> {
            // master-default repo with no remote: the shared default-base
            // authority must pick master, not fabricate origin/main
            // (#4003, RIPR-SPEC-0084).
            badge_fixture_git(&["init", "--initial-branch=master", "--quiet"])?;
            badge_fixture_base_and_edit_commits()?;
            let input = read_badge_artifact_diff_governed(root)?;
            if input.base_ref != "master" {
                return Err(format!(
                    "badge input resolved the wrong base: expected master, got {:?}",
                    input.base_ref
                ));
            }
            if !input.diff.contains("+badge-secret-line added") {
                return Err(format!(
                    "badge input must contain the branch edit; got: {}",
                    input.diff
                ));
            }
            assert_badge_commit_identity(&input.base_commit, "base")?;
            assert_badge_commit_identity(&input.head_commit, "head")?;
            assert_badge_commit_identity(&input.head_tree, "head tree")?;
            Ok(())
        },
    )
}

#[test]
fn badge_diff_reader_fails_closed_when_no_base_resolves() -> Result<(), String> {
    with_temp_cwd("badge-diff-no-base", |root| -> Result<(), String> {
        // A repository with no commits has no resolvable base. The badge
        // producer must fail with a named cause, never write an empty patch
        // that renders as a clean zero-change badge (#4003).
        badge_fixture_git(&["init", "--initial-branch=main", "--quiet"])?;
        let Err(err) = read_badge_artifact_diff_governed(root) else {
            return Err(
                "a repo with no resolvable base must fail the badge input, not return an empty diff"
                    .to_string(),
            );
        };
        if !err.contains("base") {
            return Err(format!(
                "badge input failure must name the base cause: {err}"
            ));
        }
        Ok(())
    })
}

#[test]
fn badge_diff_reader_pins_the_git_presentation_against_ambient_config() -> Result<(), String> {
    with_temp_cwd(
        "badge-diff-pinned-presentation",
        |root| -> Result<(), String> {
            badge_fixture_git(&["init", "--initial-branch=master", "--quiet"])?;
            // Ambient configuration the pre-#4003 producer absorbed: a textconv
            // driver that hides the changed source line, ANSI color, an expanded
            // ambient context, and an external diff driver that would fail
            // loudly if spawned.
            fs::write(".gitattributes", "*.txt diff=badge-wordifier\n")
                .map_err(|err| err.to_string())?;
            badge_fixture_git(&[
                "config",
                "diff.badge-wordifier.textconv",
                "sed /badge-secret-line/d",
            ])?;
            badge_fixture_git(&["config", "color.diff", "always"])?;
            badge_fixture_git(&["config", "diff.context", "8"])?;
            badge_fixture_git(&[
                "config",
                "diff.external",
                "ripr-badge-fixture-no-such-external-diff-driver",
            ])?;
            badge_fixture_base_and_edit_commits()?;
            let input = read_badge_artifact_diff_governed(root)?;
            if input.diff.contains('\u{1b}') {
                return Err(
                    "ambient color.diff=always leaked ANSI escapes into the badge input"
                        .to_string(),
                );
            }
            if !input.diff.contains("+badge-secret-line added") {
                return Err(
                    "the textconv driver hid the changed source from the badge input".to_string(),
                );
            }
            if input.diff.lines().any(|line| line.starts_with(' ')) {
                return Err(
                "ambient diff.context expanded the badge input beyond the pinned zero-context presentation"
                    .to_string(),
            );
            }
            Ok(())
        },
    )
}

#[test]
fn badge_input_identity_receipt_records_the_analyzed_subject() -> Result<(), String> {
    with_temp_cwd(
        "badge-input-identity-receipt",
        |root| -> Result<(), String> {
            badge_fixture_git(&["init", "--initial-branch=master", "--quiet"])?;
            badge_fixture_base_and_edit_commits()?;
            let input = read_badge_artifact_diff_governed(root)?;
            write_badge_input_identity(&input, "input", &[])?;
            let receipt_path = root.join("target/ripr/reports/badge-artifacts-identity.json");
            let receipt = fs::read_to_string(&receipt_path)
                .map_err(|err| format!("failed to read {}: {err}", receipt_path.display()))?;
            let value: serde_json::Value = serde_json::from_str(&receipt)
                .map_err(|err| format!("identity receipt is not valid JSON: {err}"))?;
            if value["input"]["base_ref"] != "master" {
                return Err(format!("receipt base_ref should be master: {receipt}"));
            }
            if value["input"]["diff_sha256"] != sha256_hex(input.diff.as_bytes()) {
                return Err("receipt diff digest must match the badge input bytes".to_string());
            }
            let presentation = value["input"]["presentation"]["argv"]
                .as_str()
                .unwrap_or_default();
            for pin in [
                "core.quotePath=true",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--unified=0",
                "--inter-hunk-context=0",
            ] {
                if !presentation.contains(pin) {
                    return Err(format!("receipt presentation must name the {pin} pin"));
                }
            }
            if !value["input"]["base_resolution"]
                .as_str()
                .unwrap_or_default()
                .contains("RIPR-SPEC-0084")
            {
                return Err("receipt must name the shared base resolution authority".to_string());
            }
            if value["phase"] != "input" {
                return Err(format!("unexpected receipt phase: {receipt}"));
            }
            Ok(())
        },
    )
}

#[test]
fn badge_artifacts_impl_with_runners_writes_diff_and_uses_built_binary() -> Result<(), String> {
    with_temp_cwd("badge-artifacts-impl-with-runners", |_root| {
        let diff = "diff --git a/fixture.json b/fixture.json\n+large fixture\n".to_string();
        let expected_binary = ripr_debug_binary();
        let mut build_calls = 0;
        let mut formats = Vec::new();

        badge_artifacts_impl_with_runners(
            || Ok(diff.clone()),
            |timeout| {
                build_calls += 1;
                assert_eq!(timeout.as_millis(), 90_000);
                Ok(TimedOutput {
                    status: Some(success_exit_status()),
                    stdout: String::new(),
                    stderr: String::new(),
                    duration: Duration::from_millis(12),
                    timed_out: false,
                })
            },
            |binary, format, args, timeout| {
                formats.push(format.to_string());
                assert_eq!(binary, expected_binary.as_path());
                assert_eq!(args, badge_artifact_command_args(format).as_slice());
                assert_eq!(timeout.as_millis(), 90_000);
                let stdout = match format {
                    "badge-json" => STUB_RIPR_NATIVE_JSON,
                    "badge-plus-json" => STUB_RIPR_PLUS_NATIVE_JSON,
                    _ => r#"{"label":"stub","message":"0","color":"brightgreen"}"#,
                };
                Ok(TimedOutput {
                    status: Some(success_exit_status()),
                    stdout: stdout.to_string(),
                    stderr: String::new(),
                    duration: Duration::from_millis(12),
                    timed_out: false,
                })
            },
        )?;

        assert_eq!(build_calls, 1);
        assert_eq!(
            formats,
            vec![
                "badge-json",
                "badge-shields",
                "badge-plus-json",
                "badge-plus-shields"
            ]
        );
        assert_eq!(
            fs::read_to_string("target/ripr/badge-input.diff").unwrap(),
            diff
        );
        assert!(Path::new("target/ripr/reports/ripr-badge.json").exists());
        assert!(Path::new("target/ripr/reports/ripr-plus-badge.json").exists());
        assert!(!Path::new("target/ripr/reports/badge-artifacts-limitation.json").exists());
        Ok(())
    })
}

#[test]
fn badge_artifacts_timeout_writes_named_limitation_artifacts() -> Result<(), String> {
    with_temp_cwd("badge-artifacts-timeout", |_root| {
        write(
            Path::new("target/ripr/reports/ripr-badge.json"),
            "stale badge",
        );
        write_badge_artifacts_from_diff(
            "diff --git a/fixture.json b/fixture.json\n+large fixture\n",
            Path::new("target/debug/ripr"),
            Duration::from_secs(90),
            |binary, format, args, timeout| {
                assert_eq!(binary, Path::new("target/debug/ripr"));
                assert_eq!(format, "badge-json");
                assert_eq!(timeout.as_millis(), 90_000);
                assert_eq!(
                    badge_artifact_command_label(binary, args),
                    "target/debug/ripr check --root . --diff target/ripr/badge-input.diff --format badge-json"
                );
                Ok(TimedOutput {
                    status: None,
                    stdout: "partial stdout".to_string(),
                    stderr: "still analyzing large fixture diff\n".to_string(),
                    duration: Duration::from_millis(90_001),
                    timed_out: true,
                })
            },
        )?;

        let json_path = Path::new("target/ripr/reports/badge-artifacts-limitation.json");
        let json = fs::read_to_string(json_path).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
        assert_eq!(value["status"], "warn");
        assert_eq!(value["phase"], "badge_artifacts");
        assert_eq!(value["format"], "badge-json");
        assert_eq!(
            value["limitation"]["category"],
            "badge_artifacts_diff_analysis_timeout"
        );
        assert_eq!(value["generation"]["timed_out"], true);
        assert_eq!(value["input"]["diff_bytes"], 56);
        assert_eq!(
            value["non_claims"][0],
            "no badge count claimed from this limited run"
        );

        let markdown = fs::read_to_string("target/ripr/reports/ripr-badges.md")
            .map_err(|err| err.to_string())?;
        assert!(markdown.contains("Status: warn"));
        assert!(markdown.contains("badge_artifacts_diff_analysis_timeout"));
        assert!(markdown.contains("No badge count is claimed"));
        assert!(markdown.contains("still analyzing large fixture diff"));
        assert!(
            !Path::new("target/ripr/reports/ripr-badge.json").exists(),
            "limited badge artifact generation must not leave stale diff badge output"
        );
        Ok(())
    })
}

#[test]
fn badge_artifacts_build_timeout_writes_limitation_without_running_artifacts() -> Result<(), String>
{
    with_temp_cwd("badge-artifacts-build-timeout", |_root| {
        write(
            Path::new("target/ripr/reports/ripr-badge.json"),
            "stale badge",
        );
        write_badge_artifacts_after_build(
            "diff --git a/fixture.json b/fixture.json\n+large fixture\n",
            Path::new("target/debug/ripr"),
            Duration::from_secs(90),
            |_timeout| {
                Ok(TimedOutput {
                    status: Some(failure_exit_status()),
                    stdout: String::new(),
                    stderr: "build still compiling\n".to_string(),
                    duration: Duration::from_millis(90_001),
                    timed_out: true,
                })
            },
            |_binary, _format, _args, _timeout| {
                Err("artifact runner should not run after build timeout".to_string())
            },
        )?;

        let json_path = Path::new("target/ripr/reports/badge-artifacts-limitation.json");
        let json = fs::read_to_string(json_path).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
        assert_eq!(value["format"], "badge-build");
        assert_eq!(
            value["limitation"]["category"],
            "badge_artifacts_build_timeout"
        );
        assert_eq!(
            value["limitation"]["summary"],
            "Badge artifact generation timed out while building the ripr binary before diff analysis could run."
        );
        assert!(
            value["limitation"]["repair_route"]
                .as_str()
                .unwrap_or_default()
                .contains("cargo build output")
        );
        assert!(
            !value["limitation"]["repair_route"]
                .as_str()
                .unwrap_or_default()
                .contains("narrow the diff input")
        );
        assert_eq!(value["generation"]["command"], "cargo build -p ripr");
        assert_eq!(value["generation"]["timed_out"], true);
        assert!(
            !Path::new("target/ripr/reports/ripr-badge.json").exists(),
            "limited build must not leave stale diff badge output"
        );
        Ok(())
    })
}

#[test]
fn badge_artifacts_build_failure_writes_build_incomplete_limitation() -> Result<(), String> {
    with_temp_cwd("badge-artifacts-build-failure", |_root| {
        write_badge_artifacts_after_build(
            "diff --git a/fixture.json b/fixture.json\n+large fixture\n",
            Path::new("target/debug/ripr"),
            Duration::from_secs(90),
            |_timeout| {
                Ok(TimedOutput {
                    status: Some(failure_exit_status()),
                    stdout: String::new(),
                    stderr: "build failed\n".to_string(),
                    duration: Duration::from_millis(120),
                    timed_out: false,
                })
            },
            |_binary, _format, _args, _timeout| {
                Err("artifact runner should not run after build failure".to_string())
            },
        )?;

        let json_path = Path::new("target/ripr/reports/badge-artifacts-limitation.json");
        let json = fs::read_to_string(json_path).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
        assert_eq!(value["format"], "badge-build");
        assert_eq!(
            value["limitation"]["category"],
            "badge_artifacts_build_incomplete"
        );
        assert_eq!(
            value["limitation"]["summary"],
            "Badge artifact generation could not build the ripr binary before diff analysis could run."
        );
        assert_eq!(value["generation"]["timed_out"], false);
        assert_eq!(value["generation"]["exit_code"], 1);
        assert!(
            value["limitation"]["repair_route"]
                .as_str()
                .unwrap_or_default()
                .contains("cargo build exit status")
        );
        Ok(())
    })
}

#[test]
fn badge_artifacts_successful_build_runs_direct_ripr_binary() -> Result<(), String> {
    with_temp_cwd("badge-artifacts-build-success", |_root| {
        let mut artifact_runner_called = false;
        write_badge_artifacts_after_build(
            "diff --git a/fixture.json b/fixture.json\n+large fixture\n",
            Path::new("target/debug/ripr"),
            Duration::from_secs(90),
            |_timeout| {
                Ok(TimedOutput {
                    status: Some(success_exit_status()),
                    stdout: String::new(),
                    stderr: String::new(),
                    duration: Duration::from_millis(12),
                    timed_out: false,
                })
            },
            |binary, format, args, timeout| {
                artifact_runner_called = true;
                assert_eq!(binary, Path::new("target/debug/ripr"));
                assert_eq!(format, "badge-json");
                assert_eq!(timeout.as_millis(), 90_000);
                assert_eq!(
                    badge_artifact_command_label(binary, args),
                    "target/debug/ripr check --root . --diff target/ripr/badge-input.diff --format badge-json"
                );
                Ok(TimedOutput {
                    status: None,
                    stdout: "partial stdout".to_string(),
                    stderr: "analysis timed out\n".to_string(),
                    duration: Duration::from_millis(90_001),
                    timed_out: true,
                })
            },
        )?;

        if !artifact_runner_called {
            return Err("expected successful build to run badge artifact command".to_string());
        }
        let json_path = Path::new("target/ripr/reports/badge-artifacts-limitation.json");
        let json = fs::read_to_string(json_path).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
        assert_eq!(value["format"], "badge-json");
        assert_eq!(value["generation"]["timed_out"], true);
        Ok(())
    })
}

#[test]
fn badge_artifacts_incomplete_output_names_limitation() -> Result<(), String> {
    let output = TimedOutput {
        status: None,
        stdout: "partial stdout".to_string(),
        stderr: "cargo failed before badge JSON\n".to_string(),
        duration: Duration::from_millis(12),
        timed_out: false,
    };
    let command = "cargo run -p ripr --quiet -- check --format badge-json";

    let json = limited_badge_artifacts_json(
        "badge-json",
        command,
        Duration::from_millis(90),
        17,
        &output,
    )?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["limitation"]["category"],
        "badge_artifacts_generation_incomplete"
    );
    assert_eq!(value["generation"]["timed_out"], false);
    assert_eq!(value["generation"]["stderr_bytes"], 31);
    assert_eq!(
        value["limitation"]["repair_route"],
        "inspect the badge artifact command exit status, stdout/stderr, and diff input before claiming badge counts from this run"
    );

    let markdown = limited_badge_artifacts_markdown(
        "badge-json",
        command,
        Duration::from_millis(90),
        17,
        &output,
    );
    assert!(markdown.contains("badge_artifacts_generation_incomplete"));
    assert!(markdown.contains("cargo failed before badge JSON"));
    assert!(markdown.contains("No badge count is claimed"));
    Ok(())
}

#[test]
fn badge_artifact_native_slot_maps_native_formats_only() -> Result<(), String> {
    let cases = [
        ("badge-json", Some(BadgeNativeSlot::Ripr)),
        ("badge-plus-json", Some(BadgeNativeSlot::RiprPlus)),
        ("badge-shields", None),
        ("badge-plus-shields", None),
        ("garbage", None),
    ];
    for (format, expected) in cases {
        let actual = badge_artifact_native_slot(format);
        if actual != expected {
            return Err(format!(
                "for format {format:?} expected {expected:?}, got {actual:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn badge_basis_default_derives_repo_plus_without_second_badge_job() {
    assert!(!badge_basis_needs_repo_badge_plus_job(
        &RepoBadgeArtifactOptions::default()
    ));
    assert!(badge_basis_needs_repo_badge_plus_job(
        &RepoBadgeArtifactOptions {
            gap_ledger: Some(PathBuf::from(
                "target/ripr/reports/gap-decision-ledger.json"
            )),
            repo_exposure_summary: None,
            include_seam_classes: false,
        }
    ));
}

#[test]
fn badge_basis_derived_repo_plus_keeps_canonical_headline_and_te_context() {
    let ripr = BadgeNativeAuditSnapshot {
        label: "ripr".to_string(),
        kind: "ripr".to_string(),
        scope: "repo".to_string(),
        basis: "canonical_actionable_gap".to_string(),
        message: "149".to_string(),
        status: "warn".to_string(),
        color: "orange".to_string(),
        counts: BTreeMap::from([
            ("unsuppressed_exposure_gaps".to_string(), 149),
            ("analyzed_gap_records".to_string(), 200),
        ]),
        reason_counts: BTreeMap::new(),
        warnings: vec!["advisory".to_string()],
    };
    let test_efficiency = serde_json::json!({
        "reason_counts": {
            "broad_oracle": 3
        },
        "metrics": {
            "tests_scanned": 7
        }
    });

    let ripr_plus = badge_basis_derived_ripr_plus_snapshot(&ripr, Some(&test_efficiency));

    assert_eq!(ripr_plus.label, "ripr+");
    assert_eq!(ripr_plus.kind, "ripr_plus");
    assert_eq!(ripr_plus.basis, "canonical_actionable_gap");
    assert_eq!(ripr_plus.message, "149");
    assert_eq!(
        ripr_plus.counts.get("unsuppressed_exposure_gaps"),
        Some(&149)
    );
    assert_eq!(
        ripr_plus
            .counts
            .get("unsuppressed_test_efficiency_findings"),
        Some(&0)
    );
    assert_eq!(ripr_plus.counts.get("analyzed_tests"), Some(&7));
    assert_eq!(ripr_plus.reason_counts.get("broad_oracle"), Some(&3));
    assert_eq!(ripr_plus.warnings, vec!["advisory".to_string()]);
}

#[test]
fn badge_basis_derived_repo_plus_handles_missing_test_efficiency_context() {
    let ripr = BadgeNativeAuditSnapshot {
        label: "ripr".to_string(),
        kind: "ripr".to_string(),
        scope: "repo".to_string(),
        basis: "canonical_actionable_gap".to_string(),
        message: "4".to_string(),
        status: "warn".to_string(),
        color: "orange".to_string(),
        counts: BTreeMap::from([("unsuppressed_exposure_gaps".to_string(), 4)]),
        reason_counts: BTreeMap::new(),
        warnings: Vec::new(),
    };

    let ripr_plus = badge_basis_derived_ripr_plus_snapshot(&ripr, None);

    assert_eq!(ripr_plus.label, "ripr+");
    assert_eq!(ripr_plus.kind, "ripr_plus");
    assert_eq!(ripr_plus.message, "4");
    assert_eq!(
        ripr_plus
            .counts
            .get("unsuppressed_test_efficiency_findings"),
        Some(&0)
    );
    assert_eq!(
        ripr_plus.counts.get("intentional_test_efficiency_findings"),
        Some(&0)
    );
    assert_eq!(
        ripr_plus.counts.get("suppressed_test_efficiency_findings"),
        Some(&0)
    );
    assert_eq!(ripr_plus.counts.get("unknowns_test_efficiency"), Some(&0));
    assert!(!ripr_plus.counts.contains_key("analyzed_tests"));
    assert!(ripr_plus.reason_counts.is_empty());
}

#[test]
fn badge_basis_seam_native_counts_are_internal_when_public_basis_is_canonical() {
    let ripr = BadgeNativeAuditSnapshot {
        label: "ripr".to_string(),
        kind: "ripr".to_string(),
        scope: "repo".to_string(),
        basis: "canonical_actionable_gap".to_string(),
        message: "12".to_string(),
        status: "warn".to_string(),
        color: "orange".to_string(),
        counts: BTreeMap::from([
            ("analyzed_seams".to_string(), 100),
            ("unsuppressed_exposure_gaps".to_string(), 12),
        ]),
        reason_counts: BTreeMap::new(),
        warnings: Vec::new(),
    };

    let counts = badge_basis_seam_native_counts(false, &ripr);

    assert_eq!(counts.status, "not_collected");
    assert!(counts.counts.is_empty());
    assert!(counts.note.contains("canonical_actionable_gap"));
    assert!(counts.source.contains("--include-seam-classes"));
}

#[test]
fn badge_basis_canonical_projection_names_static_repair_gaps() {
    let ripr = BadgeNativeAuditSnapshot {
        label: "ripr".to_string(),
        kind: "ripr".to_string(),
        scope: "repo".to_string(),
        basis: "canonical_actionable_gap".to_string(),
        message: "12".to_string(),
        status: "warn".to_string(),
        color: "orange".to_string(),
        counts: BTreeMap::new(),
        reason_counts: BTreeMap::new(),
        warnings: Vec::new(),
    };
    let ripr_plus = BadgeNativeAuditSnapshot {
        label: "ripr+".to_string(),
        kind: "ripr_plus".to_string(),
        scope: "repo".to_string(),
        basis: "canonical_actionable_gap".to_string(),
        message: "14".to_string(),
        status: "warn".to_string(),
        color: "orange".to_string(),
        counts: BTreeMap::new(),
        reason_counts: BTreeMap::new(),
        warnings: Vec::new(),
    };

    let projection =
        badge_basis_canonical_projection(&RepoBadgeArtifactOptions::default(), &ripr, &ripr_plus);

    assert_eq!(projection.status, "available");
    assert_eq!(projection.ripr_count, Some(12));
    assert_eq!(projection.ripr_plus_count, Some(14));
    assert!(projection.detail.contains("canonical_actionable_gap"));
    assert!(
        projection
            .detail
            .contains("unresolved actionable static repair gaps")
    );
}

#[test]
fn badge_basis_report_json_names_public_projection_rule() -> Result<(), String> {
    let report = BadgeBasisReport {
        status: "pass".to_string(),
        current_public_endpoints: vec![],
        current_repo_badges: vec![],
        seam_native_counts: BadgeCountBreakdown {
            status: "not_collected".to_string(),
            source: "badge-basis".to_string(),
            counts: BTreeMap::new(),
            note: "supporting inventory".to_string(),
        },
        test_efficiency_counts: BadgeCountBreakdown {
            status: "pass".to_string(),
            source: "test-efficiency".to_string(),
            counts: BTreeMap::new(),
            note: "available".to_string(),
        },
        canonical_actionable_gap: BadgeCanonicalProjection {
            status: "available".to_string(),
            source: "repo-badge-artifacts".to_string(),
            ripr_count: Some(3),
            ripr_plus_count: Some(3),
            detail: "canonical_actionable_gap basis".to_string(),
        },
        raw_alignment_signals: BadgeBasisSignal {
            status: "supporting".to_string(),
            source: "finding alignment".to_string(),
            count: None,
            detail: "supporting evidence".to_string(),
        },
        canonical_evidence_items: BadgeBasisSignal {
            status: "supporting".to_string(),
            source: "repo exposure".to_string(),
            count: None,
            detail: "supporting context".to_string(),
        },
        static_limitations: BadgeBasisSignal {
            status: "available".to_string(),
            source: "repo exposure".to_string(),
            count: Some(0),
            detail: "supporting inventory".to_string(),
        },
        suppressed_or_intentional_items: BadgeBasisSignal {
            status: "available".to_string(),
            source: "badge counts".to_string(),
            count: Some(0),
            detail: "out of headline".to_string(),
        },
        no_action_items: BadgeBasisSignal {
            status: "requires_gap_decision_ledger".to_string(),
            source: "none".to_string(),
            count: None,
            detail: "requires gap records".to_string(),
        },
        recommended_public_projection: "canonical_actionable_gap".to_string(),
        warnings: Vec::new(),
    };

    let json: Value =
        serde_json::from_str(&badge_basis_report_json(&report)?).map_err(|err| err.to_string())?;
    let rule = json
        .pointer("/recommended_public_projection/rule")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing recommended public projection rule".to_string())?;

    assert!(rule.contains("unresolved actionable static repair gaps"));
    assert!(rule.contains("canonical_actionable_gap"));
    assert!(rule.contains("repair, verify, and receipt model"));
    assert!(rule.contains("supporting/internal"));
    Ok(())
}

#[test]
fn badge_native_audit_snapshot_reads_basis_and_counts() -> Result<(), String> {
    let json = r#"{
  "label": "ripr",
  "kind": "ripr",
  "scope": "repo",
  "basis": "seam_native",
  "message": "42",
  "status": "warn",
  "color": "orange",
  "counts": {
    "unsuppressed_exposure_gaps": 42,
    "analyzed_seams": 99
  },
  "reason_counts": {
    "broad_oracle": 3
  },
  "warnings": ["advisory"]
}"#;
    let snapshot = badge_native_audit_snapshot(json)?;
    assert_eq!(snapshot.label, "ripr");
    assert_eq!(snapshot.scope, "repo");
    assert_eq!(snapshot.basis, "seam_native");
    assert_eq!(snapshot.counts.get("analyzed_seams"), Some(&99));
    assert_eq!(snapshot.reason_counts.get("broad_oracle"), Some(&3));
    assert_eq!(snapshot.warnings, vec!["advisory".to_string()]);
    Ok(())
}

#[test]
fn badge_basis_report_markdown_names_actionable_recommendation() {
    let report = BadgeBasisReport {
        status: "warn".to_string(),
        current_public_endpoints: vec![BadgeEndpointSnapshot {
            path: "badges/ripr.json".to_string(),
            label: "ripr".to_string(),
            message: "24352".to_string(),
            color: "orange".to_string(),
        }],
        current_repo_badges: vec![BadgeNativeAuditSnapshot {
            label: "ripr".to_string(),
            kind: "ripr".to_string(),
            scope: "repo".to_string(),
            basis: "seam_native".to_string(),
            message: "24352".to_string(),
            status: "warn".to_string(),
            color: "orange".to_string(),
            counts: BTreeMap::from([("unsuppressed_exposure_gaps".to_string(), 24352)]),
            reason_counts: BTreeMap::new(),
            warnings: Vec::new(),
        }],
        seam_native_counts: BadgeCountBreakdown {
            status: "pass".to_string(),
            source: "repo-exposure-md".to_string(),
            counts: BTreeMap::from([("weakly_gripped".to_string(), 10)]),
            note: "available".to_string(),
        },
        test_efficiency_counts: BadgeCountBreakdown {
            status: "pass".to_string(),
            source: "test-efficiency.json".to_string(),
            counts: BTreeMap::from([("likely_vacuous".to_string(), 2)]),
            note: "available".to_string(),
        },
        canonical_actionable_gap: BadgeCanonicalProjection {
            status: "not_available".to_string(),
            source: "canonical_actionable_gap generator path".to_string(),
            ripr_count: None,
            ripr_plus_count: None,
            detail: "later PR".to_string(),
        },
        raw_alignment_signals: BadgeBasisSignal {
            status: "not_in_current_badge_generator".to_string(),
            source: "finding alignment".to_string(),
            count: None,
            detail: "supporting evidence".to_string(),
        },
        canonical_evidence_items: BadgeBasisSignal {
            status: "not_in_current_badge_generator".to_string(),
            source: "repo exposure".to_string(),
            count: None,
            detail: "supporting evidence".to_string(),
        },
        static_limitations: BadgeBasisSignal {
            status: "available".to_string(),
            source: "repo exposure".to_string(),
            count: Some(1),
            detail: "supporting inventory".to_string(),
        },
        suppressed_or_intentional_items: BadgeBasisSignal {
            status: "available".to_string(),
            source: "badge counts".to_string(),
            count: Some(0),
            detail: "out of headline".to_string(),
        },
        no_action_items: BadgeBasisSignal {
            status: "requires_gap_decision_ledger".to_string(),
            source: "none".to_string(),
            count: None,
            detail: "requires gap records".to_string(),
        },
        recommended_public_projection: "canonical_actionable_gap".to_string(),
        warnings: vec!["seam-native public basis".to_string()],
    };
    let markdown = badge_basis_report_markdown(&report);
    assert!(markdown.contains("# ripr Public Badge Basis Audit"));
    assert!(markdown.contains("unresolved actionable static repair gaps"));
    assert!(markdown.contains("`seam_native`"));
    assert!(markdown.contains("`canonical_actionable_gap`"));
    assert!(markdown.contains("supporting/internal diagnostics"));
    assert!(markdown.contains("repair, verify, and receipt model"));
    assert!(markdown.contains("badges/ripr.json"));
}

#[test]
fn badge_endpoint_files_pair_committed_paths_with_repo_shields_sources() {
    // Pin both the count and the (committed -> source) mapping so a
    // future rename of either side trips the test, not the badge URL.
    assert_eq!(super::super::BADGE_ENDPOINT_FILES.len(), 2);
    let pairs: Vec<(&str, &str)> = super::super::BADGE_ENDPOINT_FILES.to_vec();
    assert!(pairs.contains(&("badges/ripr.json", "repo-ripr-badge-shields.json")));
    assert!(pairs.contains(&("badges/ripr-plus.json", "repo-ripr-plus-badge-shields.json")));
}

#[test]
fn badge_endpoint_violation_reports_missing_committed_file() -> Result<(), String> {
    let violation = super::super::badge_endpoint_violation(
        "badges/ripr.json",
        "target/ripr/reports/repo-ripr-badge-shields.json",
        b"{\"schemaVersion\":1}",
        None,
    );
    let message = violation.ok_or_else(|| "missing file should produce a violation".to_string())?;
    assert!(
        message.contains("missing badge endpoint file badges/ripr.json"),
        "violation should name the missing file: {message}"
    );
    assert!(
        message.contains("cargo xtask update-badge-endpoints"),
        "violation should point at the refresh command: {message}"
    );
    Ok(())
}

#[test]
fn badge_endpoint_violation_reports_stale_committed_file() -> Result<(), String> {
    let violation = super::super::badge_endpoint_violation(
        "badges/ripr-plus.json",
        "target/ripr/reports/repo-ripr-plus-badge-shields.json",
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"163\",\"color\":\"orange\"}",
        Some(b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"317\",\"color\":\"orange\"}"),
    );
    let message = violation.ok_or_else(|| "stale file should produce a violation".to_string())?;
    assert!(
        message.contains("badges/ripr-plus.json is stale"),
        "violation should describe the stale committed path: {message}"
    );
    assert!(
        message.contains("repo-ripr-plus-badge-shields.json"),
        "violation should name the source-of-truth file: {message}"
    );
    assert!(
        message.contains("cargo xtask update-badge-endpoints"),
        "violation should point at the refresh command: {message}"
    );
    assert!(
        message.contains("commit the diff"),
        "violation should remind the author to commit: {message}"
    );
    Ok(())
}

#[test]
fn badge_endpoint_violation_returns_none_when_committed_file_matches() {
    let bytes =
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"0\",\"color\":\"brightgreen\"}";
    let violation = super::super::badge_endpoint_violation(
        "badges/ripr.json",
        "target/ripr/reports/repo-ripr-badge-shields.json",
        bytes,
        Some(bytes),
    );
    assert!(
        violation.is_none(),
        "matching content must not produce a violation: {violation:?}"
    );
}

#[test]
fn badge_endpoint_violation_treats_byte_diff_as_stale() {
    // Even a single-byte difference (e.g. trailing newline) trips the
    // staleness check so committed files stay byte-equal with the
    // source-of-truth Shields JSON.
    let want =
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"0\",\"color\":\"brightgreen\"}\n";
    let got =
        b"{\"schemaVersion\":1,\"label\":\"ripr\",\"message\":\"0\",\"color\":\"brightgreen\"}";
    let violation = super::super::badge_endpoint_violation(
        "badges/ripr.json",
        "target/ripr/reports/repo-ripr-badge-shields.json",
        want,
        Some(got),
    );
    assert!(
        violation.is_some(),
        "trailing-byte difference must be flagged as stale"
    );
}

#[test]
fn badge_diff_policy_rejects_badge_endpoint_diff_outside_refresh_context() {
    let changes = vec![changed_path("badges/ripr-plus.json", &["M"])];
    let violations = badge_diff_policy_violations(&changes, false);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("generated badge endpoint changed"));
    assert!(violations[0].contains("badge: refresh public endpoints"));
}

#[test]
fn badge_diff_policy_allows_badge_endpoint_diff_in_refresh_context() {
    let changes = vec![changed_path("badges/ripr.json", &["M"])];
    let violations = badge_diff_policy_violations(&changes, true);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn badge_diff_policy_allows_readme_badge_layout_without_endpoint_diff() {
    let changes = vec![changed_path("README.md", &["M"])];
    let violations = badge_diff_policy_violations(&changes, false);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn badge_diff_policy_rejects_badge_endpoint_even_with_readme_change() {
    let changes = vec![
        changed_path("README.md", &["M"]),
        changed_path("badges/ripr.json", &["M"]),
    ];
    let violations = badge_diff_policy_violations(&changes, false);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("badges/ripr.json"));
}
