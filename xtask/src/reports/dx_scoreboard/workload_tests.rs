use super::*;

const WARM: &str = "speed.warm_check_ms";
const OTHER_SHA: &str = "1111111111111111111111111111111111111111";

fn warm(values: &[(&str, f64)]) -> Vec<Sample> {
    values
        .iter()
        .map(|(repo, value)| sample(WARM, Some(repo), SampleOutcome::Value(*value)))
        .collect()
}

fn report(
    config: &Config,
    samples: &[Sample],
    ctx: &RunContext,
    baseline: Option<&Value>,
) -> Value {
    build_report(
        config,
        &all_boards(),
        samples,
        ctx,
        baseline,
        baseline.is_some(),
    )
}

#[test]
fn measured_corpus_retains_the_effective_base_and_partial_contract() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let mut entry = config.corpus.first().ok_or("corpus missing")?.clone();
    let complete = measure::measured_corpus_entry(&entry, "HEAD~1");
    assert_eq!(complete["base"], "HEAD~1");
    assert_eq!(complete["language"], "rust");
    assert_eq!(complete.get("expected_partial"), Some(&Value::Null));
    assert_eq!(complete["sha"], entry.sha);
    assert_eq!(complete["url"], entry.url);

    let mut ctx = context("r");
    ctx.corpus = vec![complete];
    let samples = warm(&[("serde", 1000.0)]);
    let baseline = report(&config, &samples, &ctx, None);
    let current = report(&config, &samples, &ctx, Some(&baseline));
    assert_eq!(metric(&current, WARM)?["baseline"]["comparable"], true);

    entry.base_sha = Some(OTHER_SHA.to_string());
    entry.language = Some("typescript".to_string());
    entry.expected_partial = Some("language_scope_unsupported".to_string());
    entry.size_tier = Some("python_ts".to_string());
    let partial = measure::measured_corpus_entry(&entry, OTHER_SHA);
    assert_eq!(partial["base"], OTHER_SHA);
    assert_eq!(partial["language"], "typescript");
    assert_eq!(partial["expected_partial"], "language_scope_unsupported");
    assert_eq!(partial["size_tier"], "python_ts");
    Ok(())
}

#[test]
fn matching_workloads_preserve_both_directions_across_analyzer_revisions() -> Result<(), String> {
    for (direction, better, worse) in [
        ("lower_is_better", 900.0, 1600.0),
        ("higher_is_better", 1100.0, 400.0),
    ] {
        let mut config = parse_config(MINIMAL)?;
        config.metric.first_mut().ok_or("metric missing")?.direction = direction.to_string();
        let before = pinned_context("r", &["a"]);
        let baseline = report(&config, &warm(&[("a", 1000.0)]), &before, None);
        let mut now = pinned_context("r", &["a"]);
        now.revision = "a different analyzer revision".to_string();
        now.corpus[0]["sha"] = json!("DD18663450EC3EE38E35E3A4EE141932614E662C");
        for (value, regressed) in [(better, false), (worse, true)] {
            let current = report(&config, &warm(&[("a", value)]), &now, Some(&baseline));
            let compared = &metric(&current, WARM)?["baseline"];
            assert_eq!(compared["comparable"], true, "{direction}");
            assert_eq!(compared["regressed"], regressed, "{direction}");
            assert_eq!(
                current["gate"]["status"],
                if regressed { "fail" } else { "pass" }
            );
        }
    }
    Ok(())
}

#[test]
fn changing_any_workload_field_cannot_manufacture_a_trend() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let ctx = pinned_context("r", &["a"]);
    let baseline = report(&config, &warm(&[("a", 1000.0)]), &ctx, None);
    for (field, replacement) in [
        ("url", json!("https://example.invalid/another-repository")),
        ("sha", json!(OTHER_SHA)),
        ("base", json!(OTHER_SHA)),
        ("language", json!("typescript")),
        ("expected_partial", json!("language_scope_unsupported")),
    ] {
        let mut changed = pinned_context("r", &["a"]);
        changed.corpus[0][field] = replacement;
        for value in [1.0, 100_000.0] {
            let current = report(&config, &warm(&[("a", value)]), &changed, Some(&baseline));
            let compared = &metric(&current, WARM)?["baseline"];
            assert_eq!(compared["comparable"], false, "{field}");
            assert!(compared["delta"].is_null(), "{field}");
            assert!(compared["regressed"].is_null(), "{field}");
            let reason = format!("workload differs: {field}");
            assert_eq!(compared["uncompared_repos"][0]["reason"], reason);
            assert!(current["gate"]["uncompared"].to_string().contains(&reason));
            assert!(render_markdown(&current).contains(&reason));
        }
    }
    Ok(())
}

#[test]
fn legacy_malformed_and_duplicate_identity_never_pair_by_display_id() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let good = pinned_context("r", &["a"]);
    let valid = good.corpus.first().ok_or("identity missing")?;
    let mut invalid = vec![Vec::new(), vec![valid.clone(), valid.clone()]];
    for field in ["url", "sha", "base", "language", "expected_partial"] {
        let mut missing = valid.clone();
        assert!(
            missing
                .as_object_mut()
                .ok_or("identity is not an object")?
                .remove(field)
                .is_some()
        );
        invalid.push(vec![missing]);
    }
    for (field, value) in [
        ("url", json!(" ")),
        ("sha", json!("dd18663")),
        ("base", json!("main")),
        ("language", json!("unknown")),
        ("expected_partial", json!(false)),
        ("expected_partial", json!("made_up_limit")),
    ] {
        let mut malformed = valid.clone();
        malformed[field] = value;
        invalid.push(vec![malformed]);
    }
    let mut different = valid.clone();
    different["sha"] = json!(OTHER_SHA);
    invalid.push(vec![valid.clone(), different.clone()]);
    invalid.push(vec![different, valid.clone()]);
    for corpus in invalid {
        for invalid_baseline in [false, true] {
            let mut unknown = pinned_context("r", &["a"]);
            unknown.corpus = corpus.clone();
            let (before, now) = if invalid_baseline {
                (&unknown, &good)
            } else {
                (&good, &unknown)
            };
            let baseline = report(&config, &warm(&[("a", 1000.0)]), before, None);
            let current = report(&config, &warm(&[("a", 100_000.0)]), now, Some(&baseline));
            let compared = &metric(&current, WARM)?["baseline"];
            assert_eq!(compared["comparable"], false, "{corpus:?}");
            assert!(compared["delta"].is_null(), "{corpus:?}");
            assert_eq!(compared["uncompared_repos"][0]["repo"], "a");
        }
    }
    Ok(())
}

#[test]
fn partial_overlap_uses_only_matching_workloads_and_retains_all_exclusions() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let before = pinned_context("r", &["moved", "stable", "removed"]);
    let baseline = report(
        &config,
        &warm(&[("moved", 100_000.0), ("stable", 100.0), ("removed", 200.0)]),
        &before,
        None,
    );
    let mut now = pinned_context("r", &["moved", "stable", "added"]);
    now.corpus[0]["sha"] = json!(OTHER_SHA);
    let current = report(
        &config,
        &warm(&[("moved", 1.0), ("stable", 1000.0), ("added", 200_000.0)]),
        &now,
        Some(&baseline),
    );
    let compared = &metric(&current, WARM)?["baseline"];
    assert_eq!(compared["value"], json!(100.0));
    assert_eq!(compared["delta"], json!(900.0));
    assert_eq!(compared["regressed_repos"][0]["repo"], "stable");
    assert_eq!(
        compared["regressed_repos"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(compared["new_repos"], json!(["added"]));
    assert_eq!(compared["missing_repos"], json!(["removed"]));
    assert_eq!(compared["uncompared_repos"][0]["repo"], "moved");
    assert_eq!(current["gate"]["status"], "fail");
    assert_eq!(current["gate"]["regressions"][0]["current"], json!(1000.0));
    let rendered = render_markdown(&current);
    assert!(rendered.contains("workload differs: sha"));
    assert!(rendered.contains("not measured this run: removed"));
    Ok(())
}

#[test]
fn no_matching_workload_still_lists_new_and_missing_repositories() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let before = pinned_context("r", &["moved", "removed"]);
    let baseline = report(
        &config,
        &warm(&[("moved", 1000.0), ("removed", 1000.0)]),
        &before,
        None,
    );
    let mut now = pinned_context("r", &["moved", "added"]);
    now.corpus[0]["sha"] = json!(OTHER_SHA);
    let current = report(
        &config,
        &warm(&[("moved", 1.0), ("added", 1.0)]),
        &now,
        Some(&baseline),
    );
    let compared = &metric(&current, WARM)?["baseline"];
    assert_eq!(compared["comparable"], false);
    assert_eq!(compared["new_repos"], json!(["added"]));
    assert_eq!(compared["missing_repos"], json!(["removed"]));
    assert!(render_markdown(&current).contains("not measured this run: removed"));
    assert!(render_markdown(&current).contains("not in baseline: added"));
    Ok(())
}

#[test]
fn missing_sample_identity_cannot_fall_back_to_an_aggregate_trend() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let ctx = pinned_context("r", &["a"]);
    let baseline = report(
        &config,
        &[sample(WARM, None, SampleOutcome::Value(1000.0))],
        &ctx,
        None,
    );
    let current = report(
        &config,
        &[sample(WARM, None, SampleOutcome::Value(1.0))],
        &ctx,
        Some(&baseline),
    );
    let compared = &metric(&current, WARM)?["baseline"];
    assert_eq!(compared["comparable"], false);
    assert!(compared["delta"].is_null());
    Ok(())
}

#[test]
fn missing_or_changed_identity_cannot_hide_incomplete_measurements() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let before = pinned_context("r", &["a"]);
    let broken = [sample(WARM, Some("a"), SampleOutcome::Incomplete(1.0))];
    for prior in [SampleOutcome::Value(1000.0), SampleOutcome::Incomplete(1.0)] {
        let baseline = report(&config, &[sample(WARM, Some("a"), prior)], &before, None);
        for runner in ["r", "another-runner"] {
            for missing in [false, true] {
                let mut now = pinned_context(runner, &["a"]);
                if missing {
                    now.corpus.clear();
                } else {
                    now.corpus[0]["sha"] = json!(OTHER_SHA);
                }
                let current = report(&config, &broken, &now, Some(&baseline));
                assert_eq!(current["gate"]["status"], "fail");
                assert!(gate_failure_message(&current).contains("a: did not complete"));
                assert!(metric(&current, WARM)?["baseline"]["delta"].is_null());
            }
        }
    }
    // Existing incomplete debt is recognized only for the same known workload.
    let baseline = report(&config, &broken, &before, None);
    let unchanged = report(&config, &broken, &before, Some(&baseline));
    assert_eq!(unchanged["gate"]["status"], "pass");
    let mut moved = pinned_context("r", &["a"]);
    moved.corpus[0]["base"] = json!(OTHER_SHA);
    let recovered = report(&config, &warm(&[("a", 1000.0)]), &moved, Some(&baseline));
    assert_eq!(
        metric(&recovered, WARM)?["baseline"]["recovered_repos"],
        json!([])
    );
    Ok(())
}

#[test]
fn workload_exclusions_leave_absolute_ceilings_and_other_sources_in_force() -> Result<(), String> {
    let config = ceilinged_config("\"speed.warm_check_ms\" = 2000")?;
    let before = pinned_context("r", &["serde"]);
    let baseline = report(&config, &warm(&[("serde", 1000.0)]), &before, None);
    let mut now = pinned_context("r", &["serde"]);
    now.corpus[0]["sha"] = json!(OTHER_SHA);
    let current = report(&config, &warm(&[("serde", 3000.0)]), &now, Some(&baseline));
    assert_eq!(metric(&current, WARM)?["baseline"]["comparable"], false);
    assert_eq!(current["gate"]["status"], "fail");
    assert_eq!(
        current["gate"]["ceiling_breaches"].as_array().map(Vec::len),
        Some(1)
    );

    let mut config = parse_config(MINIMAL)?;
    let def = config
        .metric
        .iter_mut()
        .find(|m| m.id == "first_run.friction_events")
        .ok_or("metric missing")?;
    def.per_repo = true;
    let ingested = |value| {
        [sample(
            "first_run.friction_events",
            Some("a"),
            SampleOutcome::Value(value),
        )]
    };
    let baseline = report(&config, &ingested(1.0), &context("r"), None);
    let current = report(&config, &ingested(2.0), &context("r"), Some(&baseline));
    let compared = &metric(&current, "first_run.friction_events")?["baseline"];
    assert_eq!(compared["comparable"], true);
    assert_eq!(compared["regressed"], true);
    assert!(compared["uncompared_repos"].is_null());
    Ok(())
}

#[test]
fn malformed_repository_urls_are_not_numeric_workload_evidence() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    for url in [
        "https://",
        "https://:443/repo",
        "https://@/repo",
        "https://example.org:bogus/repo",
        "https://example.org:999999/repo",
        "https://[bad/repo",
        "https://example.org/a b",
        "https://%ZZ/repo",
        "https://example.org/a%ZZ",
        "file://[bad]/repo",
        "https://one@two@host/repo",
    ] {
        let mut ctx = pinned_context("r", &["a"]);
        ctx.corpus[0]["url"] = json!(url);
        let baseline = report(&config, &warm(&[("a", 1000.0)]), &ctx, None);
        let current = report(&config, &warm(&[("a", 3000.0)]), &ctx, Some(&baseline));
        let comparison = &metric(&current, WARM)?["baseline"];
        assert_eq!(comparison["comparable"], false, "{url}: {comparison}");
        assert!(comparison["delta"].is_null());
        assert!(
            comparison["uncompared_repos"][0]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("url"))
        );
        assert!(render_markdown(&current).contains("invalid `url`"));
    }
    Ok(())
}

#[test]
fn repository_identity_accepts_git_remote_and_local_forms() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    for url in [
        "https://github.com/serde-rs/serde",
        "https://git.example.org/",
        "https://git.example.org",
        "https://[::1]:443/repo",
        "https://[fe80::1%25eth0]/repo",
        "https://bücher.de/repo",
        "https://%65xample.org/repo",
        "ssh://git@example.org/group/repo.git",
        "git@example.org:group/repo.git",
        "file:///tmp/repo space.git",
        "file:///tmp/repo?#.git",
        "file:///tmp/repo%ZZ.git",
        "/tmp/repo space.git",
        "relative/repo.git",
    ] {
        let mut ctx = pinned_context("r", &["a"]);
        ctx.corpus[0]["url"] = json!(url);
        let baseline = report(&config, &warm(&[("a", 1000.0)]), &ctx, None);
        let current = report(&config, &warm(&[("a", 3000.0)]), &ctx, Some(&baseline));
        let comparison = &metric(&current, WARM)?["baseline"];
        assert_eq!(comparison["comparable"], true, "{url}: {comparison}");
        assert_eq!(comparison["delta"], json!(2000.0));
        assert_eq!(comparison["regressed"], true);
    }
    Ok(())
}
