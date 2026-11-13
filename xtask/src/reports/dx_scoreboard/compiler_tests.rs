use super::*;

const AFTER: &str =
    "rustc-v1|1.100.0|0ef1bc386059596dada8fb65b50425aab2ea637a|x86_64-unknown-linux-gnu|23.1.5";
const WARM: &str = "speed.warm_check_ms";

fn warm(value: f64) -> Vec<Sample> {
    vec![sample(WARM, Some("a"), SampleOutcome::Value(value))]
}

#[test]
fn measured_speed_compares_only_matching_recorded_compilers() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let before = pinned_context("r", &["a"]);
    let baseline = build_report(&config, &all_boards(), &warm(1000.0), &before, None, false);
    let same = build_report(
        &config,
        &all_boards(),
        &warm(3000.0),
        &before,
        Some(&baseline),
        true,
    );
    assert_eq!(metric(&same, WARM)?["baseline"]["regressed"], true);
    assert_eq!(same["gate"]["status"], "fail");

    let mut after = before.clone();
    after.rustc = Some(AFTER.to_string());
    let changed = build_report(
        &config,
        &all_boards(),
        &warm(3000.0),
        &after,
        Some(&baseline),
        true,
    );
    assert_eq!(changed["schema_version"], SCHEMA_VERSION);
    assert_eq!(changed["rustc"], AFTER);
    assert_eq!(changed["gate"]["baseline_rustc"], baseline["rustc"]);
    assert_eq!(metric(&changed, WARM)?["baseline"]["comparable"], false);
    assert!(
        metric(&changed, WARM)?["baseline"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("build compiler differs"))
    );
    assert_eq!(changed["gate"]["status"], "pass_with_gaps");
    assert!(render_markdown(&changed).contains("rustc 1.100.0"));
    Ok(())
}

#[test]
fn legacy_and_unknown_compilers_are_readable_but_never_a_numeric_speed_match() -> Result<(), String>
{
    let config = parse_config(MINIMAL)?;
    let mut current = pinned_context("r", &["a"]);
    let baseline = build_report(&config, &all_boards(), &warm(1000.0), &current, None, false);
    for identity in [
        Value::Null,
        json!(""),
        json!("unavailable"),
        json!({"wrong": "shape"}),
    ] {
        let mut old = baseline.clone();
        old["rustc"] = identity.clone();
        current.rustc = identity.as_str().map(str::to_string);
        let admitted = check_baseline(old)?;
        let report = build_report(
            &config,
            &all_boards(),
            &warm(3000.0),
            &current,
            Some(&admitted),
            true,
        );
        assert_eq!(metric(&report, WARM)?["baseline"]["comparable"], false);
        assert_ne!(report["gate"]["status"], "fail");
    }
    let mut legacy = baseline;
    legacy
        .as_object_mut()
        .ok_or("object missing")?
        .remove("rustc");
    current.rustc = None;
    let report = build_report(
        &config,
        &all_boards(),
        &warm(3000.0),
        &current,
        Some(&check_baseline(legacy)?),
        true,
    );
    assert_eq!(metric(&report, WARM)?["baseline"]["comparable"], false);
    assert!(render_markdown(&report).contains("rustc unrecorded"));
    Ok(())
}

#[test]
fn changed_compilers_preserve_completion_loss_without_cross_compiler_numeric_regressions()
-> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let mut context = pinned_context("r", &["a", "b"]);
    let baseline_samples = vec![
        sample(WARM, Some("a"), SampleOutcome::Value(1000.0)),
        sample(WARM, Some("b"), SampleOutcome::Value(1000.0)),
    ];
    let baseline = build_report(
        &config,
        &all_boards(),
        &baseline_samples,
        &context,
        None,
        false,
    );
    let current = vec![
        sample(WARM, Some("a"), SampleOutcome::Incomplete(50.0)),
        sample(WARM, Some("b"), SampleOutcome::Value(3000.0)),
    ];
    let same = build_report(
        &config,
        &all_boards(),
        &current,
        &context,
        Some(&baseline),
        true,
    );
    assert_eq!(
        metric(&same, WARM)?["baseline"]["regressed_repos"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    context.rustc = Some(AFTER.to_string());
    let changed = build_report(
        &config,
        &all_boards(),
        &current,
        &context,
        Some(&baseline),
        true,
    );
    assert_eq!(changed["gate"]["status"], "fail");
    let regressions = metric(&changed, WARM)?["baseline"]["regressed_repos"]
        .as_array()
        .ok_or("regressions missing")?;
    assert_eq!(regressions.len(), 1);
    assert_eq!(regressions[0]["repo"], "a");
    Ok(())
}

#[test]
fn compiler_boundary_does_not_waive_ceilings_or_ingested_metric_regressions() -> Result<(), String>
{
    let mut config = parse_config(MINIMAL)?;
    config.corpus[0].id = "a".to_string();
    config.corpus[0].ceilings.insert(WARM.to_string(), 1200.0);
    let mut context = pinned_context("r", &["a"]);
    let baseline = build_report(&config, &all_boards(), &warm(1000.0), &context, None, false);
    context.rustc = Some(AFTER.to_string());
    let report = build_report(
        &config,
        &all_boards(),
        &warm(3000.0),
        &context,
        Some(&baseline),
        true,
    );
    assert_eq!(metric(&report, WARM)?["baseline"]["comparable"], false);
    assert_eq!(report["gate"]["status"], "fail");
    assert_eq!(
        report["gate"]["ceiling_breaches"].as_array().map(Vec::len),
        Some(1)
    );

    config.corpus[0].ceilings.clear();
    config.metric[0].source = "ingest:first-run".to_string();
    for (board, id) in [("first_run", "first_run.walk_secs"), ("speed", WARM)] {
        config.metric[0].board = board.to_string();
        config.metric[0].id = id.to_string();
        let receipt = |value: f64| {
            json!({"schema_version": INPUT_SCHEMA_VERSION, "source": "first-run",
                "metrics": [{"id": id, "repo": "a", "value": value}]})
        };
        let before = parse_ingest(&receipt(1000.0), &config)?;
        let after = parse_ingest(&receipt(3000.0), &config)?;
        context.rustc = None;
        let baseline = build_report(&config, &all_boards(), &before, &context, None, false);
        context.rustc = Some(AFTER.to_string());
        let report = build_report(
            &config,
            &all_boards(),
            &after,
            &context,
            Some(&baseline),
            true,
        );
        assert_eq!(metric(&report, id)?["baseline"]["regressed"], true);
        assert_eq!(report["gate"]["status"], "fail");
    }
    Ok(())
}
