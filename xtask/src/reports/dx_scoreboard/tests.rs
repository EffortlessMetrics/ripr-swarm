use super::measure::{
    PasteVerdict, builds_ripr_from_source, check_contradictions, classify_replay, extract_commands,
    repo_exposure_contradictions,
};
use super::*;

/// The committed config, resolved from the crate root so a test that moves
/// the working directory cannot break the lookup.
fn committed_config() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(DEFAULT_CONFIG)
}

const MINIMAL: &str = r#"
schema_version = "ripr-dx-scoreboards-v1"

[[corpus]]
id = "serde"
url = "https://github.com/serde-rs/serde"
sha = "dd18663450ec3ee38e35e3a4ee141932614e662c"

[[metric]]
id = "speed.warm_check_ms"
board = "speed"
title = "Warm check"
unit = "ms"
direction = "lower_is_better"
target = 2000
regression_pct = 25
regression_floor = 500
runner_dependent = true
source = "measured"
per_repo = true

[[metric]]
id = "ci.workflow_lines"
board = "ci"
title = "Workflow lines"
unit = "lines"
direction = "lower_is_better"
target = 150
regression_pct = 10
regression_floor = 20
runner_dependent = false
source = "measured"

[[metric]]
id = "first_run.friction_events"
board = "first_run"
title = "Friction"
unit = "events"
direction = "lower_is_better"
target = 0
regression_pct = 0
regression_floor = 0
runner_dependent = false
source = "ingest:first-run"
"#;

fn sample(metric: &str, repo: Option<&str>, outcome: SampleOutcome) -> Sample {
    Sample {
        metric: metric.to_string(),
        repo: repo.map(str::to_string),
        outcome,
        detail: String::new(),
    }
}

fn all_boards() -> Vec<String> {
    BOARDS.iter().map(|board| (*board).to_string()).collect()
}

fn context(runner: &str) -> RunContext {
    RunContext {
        revision: "abc".to_string(),
        runner_class: runner.to_string(),
        ..RunContext::default()
    }
}

fn metric<'a>(report: &'a Value, id: &str) -> Result<&'a Value, String> {
    report["metrics"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["id"].as_str() == Some(id)))
        .ok_or_else(|| format!("metric {id} missing from report"))
}

#[test]
fn committed_scoreboards_config_parses_and_pins_full_shas() -> Result<(), String> {
    let config = load_config(&committed_config())?;
    assert!(
        config.corpus.len() >= 3,
        "corpus should name real repositories"
    );
    for board in BOARDS {
        assert!(
            config.metric.iter().any(|metric| metric.board == board),
            "board {board} has no metric"
        );
    }
    Ok(())
}

#[test]
fn config_rejects_short_sha_unknown_source_and_misprefixed_ids() {
    let short = MINIMAL.replace("dd18663450ec3ee38e35e3a4ee141932614e662c", "dd18663");
    assert!(parse_config(&short).is_err_and(|err| err.contains("40-character")));
    let source = MINIMAL.replace("source = \"ingest:first-run\"", "source = \"guess\"");
    assert!(parse_config(&source).is_err_and(|err| err.contains("unknown source")));
    let prefix = MINIMAL.replace("id = \"ci.workflow_lines\"", "id = \"workflow_lines\"");
    assert!(parse_config(&prefix).is_err_and(|err| err.contains("prefixed")));
    let duplicate = format!(
        "{MINIMAL}\n[[corpus]]\nid = \"serde\"\nurl = \"u\"\nsha = \"dd18663450ec3ee38e35e3a4ee141932614e662c\"\n"
    );
    assert!(parse_config(&duplicate).is_err_and(|err| err.contains("duplicate corpus")));
}

#[test]
fn ingest_accepts_its_own_source_and_refuses_measured_metrics() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let good = json!({
        "schema_version": INPUT_SCHEMA_VERSION,
        "source": "first-run",
        "metrics": [{"id": "first_run.friction_events", "value": 3, "evidence": "log.md"}],
    });
    let samples = parse_ingest(&good, &config)?;
    assert_eq!(samples.len(), 1);
    assert_eq!(
        samples.first().map(|s| s.outcome.clone()),
        Some(SampleOutcome::Value(3.0))
    );

    let overwrite = json!({
        "schema_version": INPUT_SCHEMA_VERSION,
        "source": "first-run",
        "metrics": [{"id": "ci.workflow_lines", "value": 1}],
    });
    assert!(parse_ingest(&overwrite, &config).is_err_and(|err| err.contains("sourced from")));

    let wrong_schema = json!({"schema_version": "v0", "source": "first-run", "metrics": []});
    assert!(
        parse_ingest(&wrong_schema, &config).is_err_and(|err| err.contains(INPUT_SCHEMA_VERSION))
    );
    Ok(())
}

#[test]
fn zero_denominator_is_not_measured_not_a_perfect_rate() {
    let json = json!({"candidates": {"false_actionable": {"numerator": 0, "denominator": 0}}});
    let empty = ratio_sample("trust.x", "f.json", "candidates.false_actionable", &json);
    assert_eq!(empty.outcome, SampleOutcome::NotMeasured);

    let json = json!({"candidates": {"false_actionable": {"numerator": 1, "denominator": 4}}});
    let rate = ratio_sample("trust.x", "f.json", "candidates.false_actionable", &json);
    assert_eq!(rate.outcome, SampleOutcome::Value(0.25));
}

#[test]
fn metric_value_is_the_worst_repo_and_incomplete_never_meets_target() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let samples = vec![
        sample(
            "speed.warm_check_ms",
            Some("a"),
            SampleOutcome::Value(900.0),
        ),
        sample(
            "speed.warm_check_ms",
            Some("b"),
            SampleOutcome::Value(2600.0),
        ),
    ];
    let report = build_report(&config, &all_boards(), &samples, &context("r"), None, false);
    let row = metric(&report, "speed.warm_check_ms")?;
    assert_eq!(row["value"].as_f64(), Some(2600.0));
    assert_eq!(row["status"].as_str(), Some("below_target"));

    let fast_but_incomplete = vec![sample(
        "speed.warm_check_ms",
        Some("a"),
        SampleOutcome::Incomplete(10.0),
    )];
    let report = build_report(
        &config,
        &all_boards(),
        &fast_but_incomplete,
        &context("r"),
        None,
        false,
    );
    assert_eq!(
        metric(&report, "speed.warm_check_ms")?["status"].as_str(),
        Some("below_target")
    );
    Ok(())
}

#[test]
fn missing_ingest_is_not_measured_and_counted_in_rollup() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let samples = vec![sample(
        "ci.workflow_lines",
        None,
        SampleOutcome::Value(120.0),
    )];
    let report = build_report(&config, &all_boards(), &samples, &context("r"), None, false);
    let friction = metric(&report, "first_run.friction_events")?;
    assert_eq!(friction["status"].as_str(), Some("not_measured"));
    assert!(
        friction["reason"]
            .as_str()
            .is_some_and(|r| r.contains("--ingest"))
    );
    assert_eq!(report["rollup"]["meets_target"].as_u64(), Some(1));
    assert_eq!(report["rollup"]["not_measured"].as_u64(), Some(2));
    Ok(())
}

#[test]
fn gate_uses_the_larger_margin_and_skips_other_runner_classes() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let baseline_samples = vec![
        sample(
            "speed.warm_check_ms",
            Some("a"),
            SampleOutcome::Value(4000.0),
        ),
        sample("ci.workflow_lines", None, SampleOutcome::Value(100.0)),
    ];
    let baseline = build_report(
        &config,
        &all_boards(),
        &baseline_samples,
        &context("runner-a"),
        None,
        false,
    );

    // 4000 -> 4900 is within 25% (1000); 100 -> 119 is within the 20-line floor.
    let within = vec![
        sample(
            "speed.warm_check_ms",
            Some("a"),
            SampleOutcome::Value(4900.0),
        ),
        sample("ci.workflow_lines", None, SampleOutcome::Value(119.0)),
    ];
    let report = build_report(
        &config,
        &all_boards(),
        &within,
        &context("runner-a"),
        Some(&baseline),
        true,
    );
    assert_eq!(report["gate"]["status"].as_str(), Some("pass"));

    // 4000 -> 5100 exceeds 25%; 100 -> 121 exceeds the floor.
    let worse = vec![
        sample(
            "speed.warm_check_ms",
            Some("a"),
            SampleOutcome::Value(5100.0),
        ),
        sample("ci.workflow_lines", None, SampleOutcome::Value(121.0)),
    ];
    let report = build_report(
        &config,
        &all_boards(),
        &worse,
        &context("runner-a"),
        Some(&baseline),
        true,
    );
    assert_eq!(report["gate"]["status"].as_str(), Some("fail"));
    assert_eq!(
        report["gate"]["regressions"].as_array().map(Vec::len),
        Some(2)
    );

    // On another runner class only the runner-independent metric is compared.
    let report = build_report(
        &config,
        &all_boards(),
        &worse,
        &context("runner-b"),
        Some(&baseline),
        true,
    );
    assert_eq!(
        report["gate"]["regressions"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        metric(&report, "speed.warm_check_ms")?["baseline"]["comparable"].as_bool(),
        Some(false)
    );
    Ok(())
}

#[test]
fn gate_fails_on_a_broken_instrument_even_without_baseline() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let samples = vec![sample("ci.workflow_lines", None, SampleOutcome::Failed)];
    let report = build_report(&config, &all_boards(), &samples, &context("r"), None, true);
    assert_eq!(report["gate"]["status"].as_str(), Some("fail"));
    let clean = vec![sample(
        "ci.workflow_lines",
        None,
        SampleOutcome::Value(10.0),
    )];
    let report = build_report(&config, &all_boards(), &clean, &context("r"), None, true);
    assert_eq!(report["gate"]["status"].as_str(), Some("no_baseline"));
    Ok(())
}

#[test]
fn repo_view_groups_per_repo_samples() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let samples = vec![sample(
        "speed.warm_check_ms",
        Some("serde"),
        SampleOutcome::Value(700.0),
    )];
    let report = build_report(&config, &all_boards(), &samples, &context("r"), None, false);
    let serde = report["repos"]
        .as_array()
        .and_then(|repos| repos.first())
        .ok_or("repo row missing")?;
    assert_eq!(serde["id"].as_str(), Some("serde"));
    assert_eq!(serde["status"].as_str(), Some("meets_target"));
    assert!(render_markdown(&report).contains("## By repository"));
    Ok(())
}

#[test]
fn extract_commands_reads_spans_labels_and_bash_fences_but_not_powershell() {
    let text = "\
ripr progress: analyzing [diff]
  ripr explain --root '/tmp/a b' --base 'HEAD~1' probe:x
  repair this seam: ripr agent repair --root '/r' --seam-id 1 --phase before
Regeneration command: `ripr check --root '/r' --format json > '/r/out.json'`
Regeneration command (PowerShell): `ripr check --root '/r''s' --format json`
- Cache size: 8 KB (run `ripr cache status` for details)
```bash
ripr agent repair --root '/fence' --phase before
```
```powershell
ripr agent repair --root '/ps''s' --phase before
```
";
    let commands = extract_commands(text);
    assert_eq!(
        commands,
        vec![
            "ripr explain --root '/tmp/a b' --base 'HEAD~1' probe:x".to_string(),
            "ripr agent repair --root '/r' --seam-id 1 --phase before".to_string(),
            "ripr check --root '/r' --format json > '/r/out.json'".to_string(),
            "ripr agent repair --root '/fence' --phase before".to_string(),
        ]
    );
}

#[test]
fn classify_replay_separates_bound_unbound_and_split_roots() {
    let root = "/tmp/x/dx paste $(touch PWNED) it's \"q\"";
    let bound = format!("explain\0--root\0{root}\0probe:x\0");
    assert_eq!(
        classify_replay(true, Some(bound.as_bytes()), Some("/tmp/caller"), root).0,
        PasteVerdict::Bound
    );
    let via_cd = b"check\0--format\0json\0";
    assert_eq!(
        classify_replay(true, Some(via_cd), Some(root), root).0,
        PasteVerdict::Bound
    );
    assert_eq!(
        classify_replay(true, Some(via_cd), Some("/tmp/caller"), root).0,
        PasteVerdict::Unbound
    );
    let split = b"check\0--root\0/tmp/x/dx\0paste\0";
    let split_root = b"check\0--root\0/tmp/x/dx paste its q\0";
    assert_eq!(
        classify_replay(true, Some(split), Some("/tmp/caller"), root).0,
        PasteVerdict::Unsafe
    );
    assert_eq!(
        classify_replay(true, Some(split_root), Some("/tmp/caller"), root).0,
        PasteVerdict::Unsafe
    );
    assert_eq!(
        classify_replay(false, None, None, root).0,
        PasteVerdict::Unsafe
    );
}

#[test]
fn contradiction_rules_fire_only_on_self_inconsistent_rows() {
    let exposure = json!({"seams": [
        {"file": "a.rs", "line": 1, "evidence": {"reach": "no"}, "related_tests_total": 81},
        {"file": "b.rs", "line": 2, "evidence": {"reach": "no"}, "related_tests_total": 0},
        {"file": "c.rs", "line": 3, "evidence": {"reach": "yes"}, "related_tests_total": 4},
    ]});
    assert_eq!(repo_exposure_contradictions(&exposure).0, 1);

    let check = json!({"findings": [
        {"id": "p1", "classification": "no_static_path", "related_tests_total": 2, "evidence": []},
        {"id": "p2", "classification": "static_unknown", "related_tests_total": 0,
         "evidence": ["Related tests were found, but no assertion appears to observe the changed value"]},
        {"id": "p3", "classification": "weakly_exposed", "related_tests_total": 3,
         "evidence": ["Related tests were found, but no assertion appears to observe the changed value"]},
    ]});
    let (count, examples) = check_contradictions(&check);
    assert_eq!(count, 2);
    assert!(examples.iter().any(|e| e.starts_with("R2 p1")));
    assert!(examples.iter().any(|e| e.starts_with("R3 p2")));
}

#[test]
fn source_build_detection_ignores_comments() {
    assert!(builds_ripr_from_source(
        "    run: cargo install ripr --version 0.11.0 --locked\n"
    ));
    assert!(!builds_ripr_from_source(
        "      # an uncached `cargo install ripr` recompiles for minutes\n"
    ));
    assert!(!builds_ripr_from_source(
        "  url=\"https://github.com/o/ripr/releases/download/v1/a.tar.gz\"\n  cargo install ripr --version 1 --locked\n"
    ));
}

#[test]
fn vm_hwm_parses_kilobytes() {
    let status = "Name:\tripr\nVmPeak:\t  900 kB\nVmHWM:\t   2048 kB\nVmRSS:\t 1000 kB\n";
    assert_eq!(crate::run::parse_vm_hwm_bytes(status), Some(2048 * 1024));
    assert_eq!(crate::run::parse_vm_hwm_bytes("Name:\tx\n"), None);
}

#[test]
fn parse_options_rejects_unknown_boards() {
    assert!(
        parse_options(&["--boards".to_string(), "speed,vibes".to_string()])
            .is_err_and(|err| err.contains("unknown board"))
    );
    let parsed = parse_options(&["--boards".to_string(), "ci,paste".to_string()]);
    assert!(parsed.is_ok_and(|options| options.boards == vec!["ci", "paste"]));
}

fn first_run_receipt(with_install: bool) -> Value {
    let mut setup = vec![json!({"step": "fetch_sources", "secs": 0.2, "exit": 0, "friction": []})];
    if with_install {
        setup.push(json!({"step": "install_published", "secs": 128.0, "exit": 0, "friction": []}));
    }
    json!({
        "schema_version": "first_run.v1",
        "ripr": "ripr 0.11.0",
        "setup": setup,
        "cases": [
            {"case": "semver", "verdict": "infection_unknown", "steps": [
                {"step": "doctor", "secs": 0.5, "exit": 0, "friction": []},
                {"step": "check", "secs": 1.5, "exit": 0, "friction": []},
                {"step": "init_ci", "secs": 0.1, "exit": 0, "friction": ["workflow is 2374 lines"]},
            ]},
            {"case": "bytesize", "verdict": "weakly_exposed", "steps": [
                {"step": "check", "secs": 2.0, "exit": 2, "friction": ["check exited 2"]},
            ]},
        ],
    })
}

#[test]
fn first_run_receipt_counts_install_through_first_successful_check() -> Result<(), String> {
    let config = parse_config(&MINIMAL.replace(
        "[[metric]]\nid = \"first_run.friction_events\"",
        "[[metric]]\nid = \"first_run.time_to_first_useful_result_s\"\nboard = \"first_run\"\ntitle = \"t\"\nunit = \"s\"\ndirection = \"lower_is_better\"\ntarget = 300\nregression_pct = 25\nregression_floor = 60\nrunner_dependent = true\nsource = \"ingest:first-run\"\n\n[[metric]]\nid = \"first_run.unknown_verdicts\"\nboard = \"first_run\"\ntitle = \"u\"\nunit = \"cases\"\ndirection = \"lower_is_better\"\ntarget = 0\nregression_pct = 0\nregression_floor = 0\nrunner_dependent = false\nsource = \"ingest:first-run\"\n\n[[metric]]\nid = \"first_run.friction_events\"",
    ))?;
    let samples = parse_ingest(&first_run_receipt(true), &config)?;
    let find = |metric: &str, repo: Option<&str>| {
        samples
            .iter()
            .find(|s| s.metric == metric && s.repo.as_deref() == repo)
            .map(|s| s.outcome.clone())
    };
    assert_eq!(
        find("first_run.time_to_first_useful_result_s", Some("semver")),
        Some(SampleOutcome::Value(130.0))
    );
    assert_eq!(
        find("first_run.time_to_first_useful_result_s", Some("bytesize")),
        Some(SampleOutcome::Incomplete(130.0))
    );
    assert_eq!(
        find("first_run.friction_events", None),
        Some(SampleOutcome::Value(2.0))
    );
    assert_eq!(
        find("first_run.unknown_verdicts", None),
        Some(SampleOutcome::Value(1.0))
    );

    // Without a timed install the journey metric stays unmeasured.
    let samples = parse_ingest(&first_run_receipt(false), &config)?;
    assert!(
        !samples
            .iter()
            .any(|s| s.metric == "first_run.time_to_first_useful_result_s")
    );
    Ok(())
}

#[test]
fn shared_corpus_manifest_maps_fast_tier_to_default_runs() -> Result<(), String> {
    let manifest = json!({
        "kind": "ripr_rust_corpus_manifest",
        "corpus_version": "2026-10-04.1",
        "repos": [
            {"id": "serde", "url": "u", "tier": "fast",
             "sha": "9d3410e3f4e38f9ea1a798e7ae9fab71577ab31b",
             "base_sha": "b7dbf7e3cb53bc9b9442047229e4f125bb07783e"},
            {"id": "tokio", "url": "u", "tier": "full",
             "sha": "9d3410e3f4e38f9ea1a798e7ae9fab71577ab31b"},
        ],
    });
    let (version, corpus) = corpus_from_manifest(&manifest)?;
    assert_eq!(version, "2026-10-04.1");
    assert_eq!(corpus.len(), 2);
    assert!(
        corpus
            .first()
            .is_some_and(|serde| !serde.heavy && serde.base_sha.is_some())
    );
    assert!(
        corpus
            .get(1)
            .is_some_and(|tokio| tokio.heavy && tokio.base_sha.is_none())
    );
    assert!(corpus_from_manifest(&json!({"kind": "other"})).is_err_and(|e| e.contains("kind")));
    Ok(())
}

#[test]
fn mutation_spot_check_receipt_maps_agreement_and_join_coverage() -> Result<(), String> {
    let receipt = json!({
        "schema_version": "ripr-mutation-spot-check-v1",
        "scored_families": {
            "claims_discriminator": {"agreement_rate": 1.0, "mutants_scored": 14},
            "claims_no_discriminator": {"agreement_rate": 0.043, "mutants_scored": 23},
        },
        "repos": [
            {"pairings": {"seam_precise": 2}, "calibration_metrics": {"mutants_total": 86}},
            {"pairings": {"seam_precise": 170}, "calibration_metrics": {"mutants_total": 1659}},
        ],
    });
    let input = mutation_spot_check_to_input(&receipt)?;
    let value = |id: &str| {
        input["metrics"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["id"].as_str() == Some(id)))
            .and_then(|row| row["value"].as_f64())
    };
    assert_eq!(value("trust.discriminator_claim_agreement"), Some(1.0));
    assert_eq!(value("trust.gap_claim_agreement"), Some(0.043));
    assert!(
        value("trust.mutation_join_coverage").is_some_and(|v| (v - 172.0 / 1745.0).abs() < 1e-9)
    );

    let config = load_config(&committed_config())?;
    let samples = parse_ingest(&receipt, &config)?;
    assert_eq!(samples.len(), 3);
    Ok(())
}

#[test]
fn first_run_rows_map_to_gates_and_list_verdicts() {
    let text = [
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"_setup","step":"install_published","metric":"secs","value":40.0,"budget":null,"better":"lower"}"#,
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"a","step":"doctor","metric":"secs","value":0.5,"budget":5,"better":"lower"}"#,
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"a","step":"check","metric":"secs","value":12.0,"budget":10,"better":"lower"}"#,
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"a","step":"check","metric":"exit","value":0,"budget":0,"better":"equal"}"#,
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"a","step":"check","metric":"verdict","value":"infection_unknown","budget":null,"better":"review_on_change"}"#,
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"a","step":"pilot","metric":"secs","value":1.0,"budget":30,"better":"lower"}"#,
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"a","step":"pilot","metric":"exit","value":2,"budget":0,"better":"equal"}"#,
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"a","step":"init_ci","metric":"workflow_lines","value":2374,"budget":1500,"better":"lower"}"#,
        r#"{"schema":"first_run_row.v1","ripr":"ripr 0.11.0","case":"a","step":"init_ci","metric":"friction_count","value":2,"budget":0,"better":"lower"}"#,
    ]
    .join("\n");
    let value = parse_ingest_text(&text).unwrap_or_default();
    let converted = first_run_rows_to_input(&value).unwrap_or_default();
    let get = |id: &str| -> Vec<Value> {
        converted["metrics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["id"].as_str() == Some(id))
            .cloned()
            .collect()
    };
    assert_eq!(get("first_run.failed_steps")[0]["value"], json!(1));
    assert_eq!(get("first_run.over_budget_steps")[0]["value"], json!(2));
    assert_eq!(get("first_run.friction_events")[0]["value"], json!(2.0));
    assert_eq!(get("first_run.unknown_verdicts")[0]["value"], json!(1));
    assert_eq!(
        get("first_run.unknown_verdicts")[0]["evidence"],
        json!("verdicts: a=infection_unknown")
    );
    assert_eq!(get("first_run.walk_secs")[0]["value"], json!(13.5));
    // install 40 + doctor 0.5 + check 12 = first useful result at 52.5 s
    let first = &get("first_run.time_to_first_useful_result_s")[0];
    assert_eq!(first["value"], json!(52.5));
    assert_eq!(first["completed"], json!(true));
    assert!(
        parse_ingest_text("{\"schema\":\"other\"}\nnot json")
            .is_err_and(|err| err.contains("line 1"))
    );
}

#[test]
fn losing_completion_regresses_on_any_runner_class() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let base = vec![sample(
        "speed.warm_check_ms",
        Some("a"),
        SampleOutcome::Value(1000.0),
    )];
    let baseline = build_report(&config, &all_boards(), &base, &context("runner-a"), None, false);
    // A check that now exits early looks faster than the baseline.
    let broken = vec![sample(
        "speed.warm_check_ms",
        Some("a"),
        SampleOutcome::Incomplete(50.0),
    )];
    for runner in ["runner-a", "runner-b"] {
        let report = build_report(
            &config,
            &all_boards(),
            &broken,
            &context(runner),
            Some(&baseline),
            true,
        );
        assert_eq!(report["gate"]["status"].as_str(), Some("fail"), "{runner}");
    }
    Ok(())
}

#[test]
fn gate_lists_baseline_metrics_it_could_not_compare() -> Result<(), String> {
    let config = parse_config(MINIMAL)?;
    let base = vec![
        sample("ci.workflow_lines", None, SampleOutcome::Value(100.0)),
        sample(
            "speed.warm_check_ms",
            Some("a"),
            SampleOutcome::Value(1000.0),
        ),
    ];
    let baseline = build_report(&config, &all_boards(), &base, &context("r"), None, false);
    let current = vec![sample("ci.workflow_lines", None, SampleOutcome::Value(100.0))];
    let report = build_report(
        &config,
        &all_boards(),
        &current,
        &context("r"),
        Some(&baseline),
        true,
    );
    assert_eq!(report["gate"]["status"].as_str(), Some("pass"));
    let uncompared = report["gate"]["uncompared"]
        .as_array()
        .ok_or("uncompared list missing")?;
    assert!(
        uncompared
            .iter()
            .any(|item| item["metric"] == "speed.warm_check_ms")
    );
    assert!(render_markdown(&report).contains("Not compared with the baseline"));
    Ok(())
}

#[test]
fn manifest_corpus_entries_must_pin_full_shas_and_unique_ids() {
    let entry = |id: &str, sha: &str| CorpusEntry {
        id: id.to_string(),
        url: "u".to_string(),
        sha: sha.to_string(),
        base_sha: None,
        note: String::new(),
        heavy: false,
    };
    let full = "9d3410e3f4e38f9ea1a798e7ae9fab71577ab31b";
    assert!(validate_corpus(&[entry("a", full)]).is_ok());
    assert!(validate_corpus(&[entry("a", "main")]).is_err_and(|e| e.contains("40-character")));
    assert!(
        validate_corpus(&[entry("a", full), entry("a", full)])
            .is_err_and(|e| e.contains("duplicate"))
    );
}
