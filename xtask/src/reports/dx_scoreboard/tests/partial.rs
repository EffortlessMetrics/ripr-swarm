use super::*;

#[test]
fn qualifying_partial_measurement_is_visible_without_changing_the_gate() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let boards = vec!["speed".to_string()];
    let baseline_samples = vec![
        sample(
            "speed.warm_check_ms",
            Some("serde"),
            SampleOutcome::Value(1000.0),
        ),
        sample(
            "speed.warm_check_ms",
            Some("beta"),
            SampleOutcome::Value(1500.0),
        ),
    ];
    let baseline = build_report(
        &config,
        &boards,
        &baseline_samples,
        &context("same-runner"),
        None,
        false,
    );
    let complete = metric(&baseline, "speed.warm_check_ms")?;
    assert_eq!(complete["value"], json!(1500.0));
    assert_eq!(complete["status"], "meets_target");
    assert_eq!(complete["partial"], false);
    assert_eq!(baseline["rollup"]["meets_target"], 1);
    assert_eq!(baseline["rollup"]["partial"], 0);
    assert_eq!(baseline["boards"][0]["status"], "meets_target");
    assert!(!render_markdown(&baseline).contains("| Partial |"));

    let samples = vec![
        sample(
            "speed.warm_check_ms",
            Some("serde"),
            SampleOutcome::Value(1000.0),
        ),
        sample(
            "speed.warm_check_ms",
            Some("beta"),
            SampleOutcome::NotMeasured,
        ),
    ];
    let report = build_report(
        &config,
        &boards,
        &samples,
        &context("same-runner"),
        Some(&baseline),
        true,
    );
    let row = metric(&report, "speed.warm_check_ms")?;
    assert_eq!(row["value"], json!(1000.0));
    assert_eq!(
        row["status"], "meets_target",
        "retain the metric's target status"
    );
    assert_eq!(row["partial"], true);
    assert_eq!(report["rollup"]["meets_target"], 0);
    assert_eq!(report["rollup"]["partial"], 1);
    assert_eq!(report["rollup"]["not_measured"], 0);
    assert_eq!(report["boards"][0]["status"], "partial");
    assert_eq!(
        report["gate"]["status"], "pass_with_gaps",
        "coverage disclosure is not a new gate failure"
    );
    assert_eq!(row["baseline"]["delta"], json!(0.0));
    assert!(
        report["gate"]["uncompared"]
            .as_array()
            .ok_or("missing uncompared rows")?
            .iter()
            .any(|item| {
                item["metric"] == "speed.warm_check_ms"
                    && item["reason"]
                        .as_str()
                        .is_some_and(|reason| reason.contains("beta"))
            })
    );
    let serde = report["repos"]
        .as_array()
        .ok_or("missing repository rows")?
        .iter()
        .find(|repo| repo["id"] == "serde")
        .ok_or("serde row missing")?;
    assert_eq!(
        serde["status"], "meets_target",
        "a measured repository keeps its own status"
    );

    let markdown = render_markdown(&report);
    assert!(
        markdown.contains("0 meet target, 1 partially measured"),
        "{markdown}"
    );
    assert!(
        markdown.contains("| speed | partial | 0 | 1 | 0 | 0 |"),
        "{markdown}"
    );
    assert!(
        markdown.contains("meets_target (partially measured)"),
        "{markdown}"
    );

    let regressing_samples = vec![
        sample(
            "speed.warm_check_ms",
            Some("serde"),
            SampleOutcome::Value(1900.0),
        ),
        sample(
            "speed.warm_check_ms",
            Some("beta"),
            SampleOutcome::NotMeasured,
        ),
    ];
    let regressing = build_report(
        &config,
        &boards,
        &regressing_samples,
        &context("same-runner"),
        Some(&baseline),
        true,
    );
    assert_eq!(regressing["rollup"]["partial"], 1);
    assert_eq!(regressing["rollup"]["regressed"], 1);
    assert_eq!(regressing["boards"][0]["status"], "attention");
    assert_eq!(regressing["gate"]["status"], "fail");
    Ok(())
}

#[test]
fn partial_coverage_keeps_below_failed_and_unmeasured_precedence() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let boards = vec!["speed".to_string()];
    for (outcomes, status, board_status, partial, gate_status) in [
        (
            vec![SampleOutcome::NotMeasured],
            "not_measured",
            "not_measured",
            false,
            None,
        ),
        (
            vec![SampleOutcome::Value(2500.0), SampleOutcome::NotMeasured],
            "below_target",
            "gaps",
            true,
            Some("no_baseline"),
        ),
        (
            vec![SampleOutcome::Incomplete(10.0), SampleOutcome::NotMeasured],
            "below_target",
            "gaps",
            true,
            Some("no_baseline"),
        ),
        (
            vec![
                SampleOutcome::Value(1000.0),
                SampleOutcome::Failed,
                SampleOutcome::NotMeasured,
            ],
            "failed",
            "attention",
            true,
            Some("fail"),
        ),
    ] {
        let samples: Vec<Sample> = outcomes
            .into_iter()
            .map(|outcome| sample("speed.warm_check_ms", None, outcome))
            .collect();
        let report = build_report(&config, &boards, &samples, &context("r"), None, true);
        let row = metric(&report, "speed.warm_check_ms")?;
        assert_eq!(row["status"], status);
        assert_eq!(row["partial"], partial);
        assert_eq!(report["boards"][0]["status"], board_status);
        assert_eq!(report["rollup"][status], 1);
        assert_eq!(report["rollup"]["meets_target"], 0);
        assert_eq!(
            report["rollup"]["partial"], 0,
            "count only qualifying partial metrics"
        );
        if let Some(gate_status) = gate_status {
            assert_eq!(report["gate"]["status"], gate_status);
        }
        let markdown = render_markdown(&report);
        if partial {
            assert!(row["value"].is_number());
            assert!(
                markdown.contains(&format!("{status} (partially measured)")),
                "{markdown}"
            );
        } else {
            assert_eq!(row["value"], Value::Null);
            assert!(!markdown.contains("partially measured"), "{markdown}");
        }
    }
    Ok(())
}
