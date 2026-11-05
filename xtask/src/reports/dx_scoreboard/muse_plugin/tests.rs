use super::super::{
    Config, RunContext, Sample, SampleOutcome, build_report, load_ingest, parse_config,
    parse_ingest, parse_ingest_text,
};
use super::*;
use std::path::Path;

const FIXTURE: &str =
    include_str!("../../../../../tests/fixtures/verification/ripr/muse-plugin-eval.valid.json");

fn config() -> Result<Config, String> {
    parse_config(include_str!(
        "../../../../../benchmarks/dx_scoreboard/scoreboards.toml"
    ))
}

fn fixture() -> Result<Value, String> {
    serde_json::from_str(FIXTURE).map_err(|err| err.to_string())
}

fn sparse_fixture() -> Result<Value, String> {
    let mut value = fixture()?;
    value
        .as_object_mut()
        .ok_or("canonical fixture")?
        .remove("observations");
    value["model"] = Value::Null;
    value["repair"] = Value::Null;
    value["execution"] = json!({"commands": [
        {"command": "cargo test", "exit_status": 0, "role": "positive"},
        {"command": "cargo test", "exit_status": 0, "role": "wrong_implementation_control"},
        {"command": "cargo test", "exit_status": 0, "role": "restore"},
    ]});
    value["verdicts"] = json!({
        "static_verdict": "exposed", "executed_verdict": "reported passing",
        "combined_claim": "completed_repair",
    });
    value["efficiency"] = json!({
        "model_calls": 4, "tool_calls": 8, "input_tokens": 100, "output_tokens": 5,
        "wall_time": "not_measured", "peak_memory": "unknown", "reported_cost": "not_measured",
    });
    Ok(value)
}

fn full_fixture() -> Result<Value, String> {
    let mut value = fixture()?;
    let artifacts = value["observations"]["evidence"]
        .as_array_mut()
        .ok_or("fixture evidence")?;
    for (id, kind, digit) in [
        ("static-before", "static_report", "a"),
        ("static-after", "static_report", "b"),
        ("durable-before", "repair_manifest", "c"),
        ("durable-after", "repair_manifest", "d"),
        ("original-log", "test_execution", "e"),
        ("counterexample-log", "test_execution", "f"),
        ("restored-log", "test_execution", "7"),
        ("usage-log", "usage_report", "8"),
    ] {
        artifacts.push(json!({
            "id": id, "kind": kind,
            "uri": format!("test-fixture://not-a-measured-run/{id}"),
            "sha256": digit.repeat(64),
        }));
    }
    value["observations"]["static"] = json!({
        "gap_id": "synthetic:boundary",
        "before": "weakly_exposed", "before_evidence": "static-before",
        "after": "exposed", "after_evidence": "static-after",
    });
    value["observations"]["durable"] = json!({
        "attempt_id": "repair-attempt-0123456789abcdef01234567",
        "before_state": "awaiting_edit", "before_manifest": "durable-before",
        "after_manifest": "durable-after", "after_state": "ready_to_finish",
        "current": true, "edit_cage": "compliant",
    });
    let execution = |id: &str, implementation: &str| {
        json!({
            "id": id, "evidence": format!("{id}-log"),
            "implementation_sha256": implementation.repeat(64),
            "tests_sha256": "9".repeat(64),
            "status": "passed", "tests_executed": 1, "exit_code": 0,
        })
    };
    value["observations"]["executions"] = json!({
        "test_selector": "fixture_boundary",
        "command": ["cargo", "test", "--", "--exact", "fixture_boundary"],
        "original": execution("original", "1"),
        "counterexample": execution("counterexample", "2"),
        "restored": execution("restored", "1"),
    });
    value["observations"]["executions"]["counterexample"]["status"] = json!("failed");
    value["observations"]["executions"]["counterexample"]["exit_code"] = json!(101);
    value["observations"]["executions"]["counterexample"]["failure_kind"] = json!("assertion");
    value["observations"]["executions"]["counterexample"]["failed_test"] =
        json!("fixture_boundary");
    value["observations"]["usage"] = json!({
        "complete": true, "evidence": "usage-log",
        "model_calls": 4, "tool_calls": 8, "elapsed_ms": 50.5, "peak_rss_bytes": 2048,
        "tokens": {"input": 100, "cached_input": 80, "output": 5, "total": 105},
    });
    Ok(value)
}

fn observed<'a>(samples: &'a [Sample], name: &str) -> Result<&'a SampleOutcome, String> {
    samples
        .iter()
        .find(|sample| sample.metric == format!("agent.muse_{name}"))
        .map(|sample| &sample.outcome)
        .ok_or_else(|| format!("missing Muse sample {name}"))
}

fn report_metric<'a>(report: &'a Value, name: &str) -> Result<&'a Value, String> {
    report["metrics"]
        .as_array()
        .ok_or("report metrics")?
        .iter()
        .find(|row| row["id"] == format!("agent.muse_{name}"))
        .ok_or_else(|| format!("missing report metric {name}"))
}

#[test]
fn native_muse_fixture_routes_without_inventing_repair() -> Result<(), String> {
    let config = config()?;
    let value = parse_ingest_text(FIXTURE)?;
    let samples = parse_ingest(&value, &config)?;
    assert_eq!(
        observed(&samples, "setup_passed")?,
        &SampleOutcome::Value(1.0)
    );
    assert_eq!(
        observed(&samples, "durable_ready_reported")?,
        &SampleOutcome::NotMeasured
    );
    assert_eq!(
        observed(&samples, "executed_discrimination")?,
        &SampleOutcome::NotMeasured
    );
    assert_eq!(
        observed(&samples, "input_tokens")?,
        &SampleOutcome::NotMeasured
    );
    assert_eq!(samples.len(), METRICS.len());
    assert_eq!(
        samples
            .iter()
            .filter(|sample| sample.detail.contains("supplied_record="))
            .count(),
        1
    );
    assert!(
        samples
            .iter()
            .any(|sample| sample.detail.contains("plugin_package_sha256"))
    );
    let report = build_report(
        &config,
        &["agent".to_string()],
        &samples,
        &RunContext::default(),
        None,
        false,
    );
    let absent = report_metric(&report, "executed_discrimination")?;
    assert_eq!(absent["status"], "not_measured");
    assert_eq!(absent["value"], Value::Null);
    assert!(
        !absent["reason"]
            .as_str()
            .ok_or("reason")?
            .contains("no --ingest")
    );
    Ok(())
}

#[test]
fn native_muse_sparse_record_retains_context_without_measurements() -> Result<(), String> {
    let config = config()?;
    let value = sparse_fixture()?;
    let encoded = serde_json::to_string(&value).map_err(|err| err.to_string())?;
    let samples = parse_ingest(&parse_ingest_text(&encoded)?, &config)?;
    assert_eq!(samples.len(), METRICS.len());
    assert!(samples.iter().all(|sample| {
        sample.outcome == SampleOutcome::NotMeasured
            && sample
                .repo
                .as_deref()
                .is_some_and(|id| id.starts_with("muse-document:"))
            && !sample.detail.contains("run_id=")
    }));
    assert_eq!(
        samples
            .iter()
            .filter(|sample| sample.detail.contains("supplied_record="))
            .count(),
        1
    );
    assert!(
        samples
            .iter()
            .any(|sample| sample.detail.contains("completed_repair"))
    );
    let report = build_report(
        &config,
        &["agent".to_string()],
        &samples,
        &RunContext::default(),
        None,
        false,
    );
    for name in METRICS {
        let metric = report_metric(&report, name)?;
        assert_eq!(metric["status"], "not_measured");
        assert_eq!(metric["value"], Value::Null);
        assert!(
            !metric["reason"]
                .as_str()
                .ok_or("reason")?
                .contains("no --ingest")
        );
    }
    for omitted in [false, true] {
        let mut partial = value.clone();
        for key in ["model", "repair", "execution"] {
            if omitted {
                partial.as_object_mut().ok_or("record")?.remove(key);
            } else {
                partial[key] = Value::Null;
            }
        }
        assert!(
            parse_ingest(&partial, &config)?
                .iter()
                .all(|sample| sample.outcome == SampleOutcome::NotMeasured)
        );
    }
    Ok(())
}

#[test]
fn native_muse_envelope_joins_pins_without_aliasing_namespaces() -> Result<(), String> {
    let mut base = full_fixture()?;
    base["repair"] = json!({
        "canonical_id": "synthetic:boundary", "seam_id": "a-separate-seam-identity",
        "attempt_id": "repair-attempt-0123456789abcdef01234567",
        "before_class": "weakly_exposed", "after_class": "exposed",
    });
    base["mcp"]["fresh_for_working_tree"] = json!(false);
    base["identity"]["fixture_digest"] = json!(format!("sha256:{}", "5".repeat(64)));
    assert_ne!(
        base["environment"]["git_head"],
        base["identity"]["ripr_commit"]
    );
    to_input(&base)?;
    for (pointer, replacement) in [
        ("/identity/scenario_id", json!("another-scenario")),
        ("/identity/fixture_digest", json!("0".repeat(64))),
        ("/identity/ripr_commit", json!("0".repeat(40))),
        ("/identity/ripr_version", json!("another-version")),
        ("/identity/plugin_version", json!("another-version")),
        ("/identity/muse_version", json!("another-version")),
        ("/environment/os", json!("another-platform")),
        ("/environment/ripr_binary", json!("missing")),
        ("/environment/ripr_binary", json!("predates_mcp")),
        ("/mcp/setup_outcome", json!("approval_required")),
        ("/mcp/setup_outcome", json!("server_unavailable")),
        ("/model/model", json!("another-model")),
        ("/model/trial_id", json!("another-trial")),
        ("/model/configuration", json!("changed configuration bytes")),
        ("/task/initial_request", json!("changed task bytes")),
        ("/repair/canonical_id", json!("another-gap")),
        (
            "/repair/attempt_id",
            json!("repair-attempt-000000000000000000000000"),
        ),
        ("/repair/before_class", json!("exposed")),
        ("/repair/after_class", json!("weakly_exposed")),
    ] {
        let mut bad = base.clone();
        *bad.pointer_mut(pointer).ok_or(pointer)? = replacement;
        let err = to_input(&bad)
            .err()
            .ok_or_else(|| format!("accepted {pointer}"))?;
        assert!(err.contains("canonical"), "{pointer}: {err}");
    }
    // Null/missing envelope fields do not erase supplied structured pins.
    base["identity"]["ripr_commit"] = Value::Null;
    base["model"] = Value::Null;
    base["repair"] = Value::Null;
    to_input(&base)?;
    base["observations"]["schema_version"] = json!(SCHEMA_VERSION);
    to_input(&base).err().ok_or("nested version accepted")?;
    let mut failed = fixture()?;
    failed["observations"]["setup"]["status"] = json!("failed");
    failed["observations"]["setup"]["plugin_loaded"] = json!(false);
    failed["observations"]["setup"]["mcp_available"] = json!(false);
    let err = to_input(&failed).err().ok_or("ready setup accepted failed MCP")?;
    assert!(err.contains("canonical setup context"), "{err}");
    failed["mcp"]["setup_outcome"] = json!("server_unavailable");
    to_input(&failed)?;
    Ok(())
}

#[test]
fn native_muse_completion_cannot_override_refuted_observations() -> Result<(), String> {
    let mut base = full_fixture()?;
    base["verdicts"]["combined_claim"] = json!("completed_repair");
    to_input(&base)?;
    let mut wrong_passes = base.clone();
    let wrong = wrong_passes["observations"]["executions"]["counterexample"]
        .as_object_mut()
        .ok_or("counterexample")?;
    wrong.insert("status".to_string(), json!("passed"));
    wrong.insert("exit_code".to_string(), json!(0));
    wrong.remove("failure_kind");
    wrong.remove("failed_test");
    let mut stale = base.clone();
    stale["observations"]["durable"]["after_state"] = json!("stale");
    stale["observations"]["durable"]["current"] = json!(false);
    for bad in [wrong_passes, stale] {
        let err = to_input(&bad).err().ok_or("refuted completion accepted")?;
        assert!(err.contains("completed_repair"), "{err}");
    }
    for name in ["original", "restored", "counterexample"] {
        let mut failed = full_fixture()?;
        let leg = &mut failed["observations"]["executions"][name];
        leg["status"] = json!("failed");
        leg["exit_code"] = json!(101);
        if name == "counterexample" {
            leg["failure_kind"] = json!("compile");
            leg["tests_executed"] = json!(0);
            leg.as_object_mut()
                .ok_or("execution leg")?
                .remove("failed_test");
        } else {
            leg["failure_kind"] = json!("assertion");
            leg["failed_test"] = json!("fixture_boundary");
        }
        to_input(&failed)?;
        failed["verdicts"]["combined_claim"] = json!("completed_repair");
        let err = to_input(&failed).err().ok_or("failed completion accepted")?;
        assert!(err.contains("completed_repair"), "{name}: {err}");
    }
    for class in ["weakly_exposed", "reachable_unrevealed", "no_static_path"] {
        let mut negative = base.clone();
        negative["observations"]["static"]["after"] = json!(class);
        let err = to_input(&negative)
            .err()
            .ok_or("known-negative static completion accepted")?;
        assert!(err.contains("completed_repair"), "{class}: {err}");
        negative["verdicts"]["combined_claim"] = json!("not_completed");
        assert_eq!(
            observed(
                &parse_ingest(&negative, &config()?)?,
                "static_exposed_after"
            )?,
            &SampleOutcome::Value(0.0)
        );
    }
    base["observations"]["executions"]["restored"]["status"] = json!("incomplete");
    let samples = parse_ingest(&base, &config()?)?;
    assert_eq!(
        observed(&samples, "executed_discrimination")?,
        &SampleOutcome::NotMeasured
    );
    Ok(())
}

#[test]
fn native_muse_unknown_static_keeps_executed_evidence_independent() -> Result<(), String> {
    let config = config()?;
    for class in ["infection_unknown", "propagation_unknown", "static_unknown"] {
        let mut value = full_fixture()?;
        value["verdicts"]["combined_claim"] = json!("completed_repair");
        value["observations"]["static"]["after"] = json!(class);
        let samples = parse_ingest(&value, &config)?;
        assert_eq!(samples.len(), METRICS.len());
        assert_eq!(
            observed(&samples, "static_exposed_after")?,
            &SampleOutcome::NotMeasured
        );
        assert_eq!(
            observed(&samples, "executed_discrimination")?,
            &SampleOutcome::Value(1.0)
        );
        assert!(
            samples
                .iter()
                .all(|sample| sample.metric != "agent.muse_completed_repair")
        );
        let retained = samples
            .iter()
            .find_map(|sample| sample.detail.split_once("; supplied_record="))
            .map(|(_, record)| record)
            .ok_or("supplied canonical record missing")?;
        let retained: Value = serde_json::from_str(retained).map_err(|err| err.to_string())?;
        assert_eq!(retained["observations"]["static"]["after"], class);
        let report = build_report(
            &config,
            &["agent".to_string()],
            &samples,
            &RunContext::default(),
            None,
            false,
        );
        let static_metric = report_metric(&report, "static_exposed_after")?;
        assert_eq!(static_metric["status"], "not_measured");
        assert_eq!(static_metric["value"], Value::Null);
        assert_eq!(
            report_metric(&report, "executed_discrimination")?["value"],
            1.0
        );
        for key in ["setup", "durable", "executions", "usage"] {
            value["observations"]
                .as_object_mut()
                .ok_or("observations")?
                .remove(key);
        }
        let unknown_only = parse_ingest(&value, &config)?;
        assert_eq!(unknown_only.len(), METRICS.len());
        assert!(
            unknown_only
                .iter()
                .all(|sample| sample.outcome == SampleOutcome::NotMeasured)
        );
    }
    Ok(())
}

#[test]
fn native_muse_envelope_usage_checks_units_without_parsing_unknown_strings() -> Result<(), String> {
    let mut value = full_fixture()?;
    value["observations"]["usage"]["elapsed_ms"] = json!(1500);
    value["efficiency"] = json!({
        "wall_time": 1.5, "model_calls": 4, "tool_calls": 8,
        "input_tokens": 100, "output_tokens": 5,
    });
    to_input(&value)?;
    for pointer in [
        "/efficiency/wall_time",
        "/efficiency/model_calls",
        "/efficiency/tool_calls",
        "/efficiency/input_tokens",
        "/efficiency/output_tokens",
    ] {
        let mut bad = value.clone();
        *bad.pointer_mut(pointer).ok_or(pointer)? = json!(999);
        assert!(to_input(&bad).is_err(), "accepted {pointer}");
    }
    for opaque in ["not_measured", "unknown", "1500"] {
        value["efficiency"]["wall_time"] = json!(opaque);
        assert_eq!(
            observed(&parse_ingest(&value, &config()?)?, "elapsed_ms")?,
            &SampleOutcome::Value(1500.0)
        );
    }
    Ok(())
}

#[test]
fn native_muse_full_trial_keeps_distinct_axes_and_exact_token_arithmetic() -> Result<(), String> {
    let value = full_fixture()?;
    let config = config()?;
    let samples = parse_ingest(&value, &config)?;
    assert_eq!(samples.len(), 16);
    for name in &METRICS[..7] {
        assert_eq!(
            observed(&samples, name)?,
            &SampleOutcome::Value(1.0),
            "{name}"
        );
    }
    assert_eq!(
        observed(&samples, "input_tokens")?,
        &SampleOutcome::Value(100.0)
    );
    assert_eq!(
        observed(&samples, "cached_input_tokens")?,
        &SampleOutcome::Value(80.0)
    );
    assert_eq!(
        observed(&samples, "uncached_input_tokens")?,
        &SampleOutcome::Value(20.0)
    );
    assert_eq!(
        observed(&samples, "output_tokens")?,
        &SampleOutcome::Value(5.0)
    );
    assert_eq!(
        observed(&samples, "total_tokens")?,
        &SampleOutcome::Value(105.0)
    );
    assert_eq!(parse_ingest(&value, &config)?, samples);
    for name in &METRICS[7..] {
        let id = format!("agent.muse_{name}");
        let metric = config
            .metric
            .iter()
            .find(|metric| metric.id == id)
            .ok_or(id)?;
        assert_eq!(metric.target.to_bits(), 0.0_f64.to_bits());
        assert_eq!(metric.direction, "higher_is_better");
        assert!(metric.review_on_change);
        assert_eq!(metric.source, "ingest:muse-plugin");
    }
    Ok(())
}

#[test]
fn native_muse_schema_rejects_pin_shape_and_numeric_drift() -> Result<(), String> {
    let base = full_fixture()?;
    to_input(&base)?;
    let cases = [
        (
            "/observations/subject/plugin_package_sha256",
            json!("unpinned"),
        ),
        ("/observations/subject/ripr_commit", json!("main")),
        ("/observations/subject/model_config_sha256", Value::Null),
        ("/observations/trial_id", json!("")),
        ("/observations/recorded_unix_ms", json!(-1)),
        ("/observations/usage/model_calls", json!(0.5)),
        (
            "/observations/usage/tool_calls",
            json!(9007199254740992_u64),
        ),
        ("/observations/usage/elapsed_ms", json!(-1.0)),
        ("/observations/usage/peak_rss_bytes", Value::Null),
        ("/observations/durable/attempt_id", json!("narrated repair")),
        ("/observations/static/after", json!("killed")), // ripr-allow: static-language: rejection control for an invalid runtime label
        ("/observations/executions/command", json!([])),
    ];
    for (pointer, replacement) in cases {
        let mut bad = base.clone();
        *bad.pointer_mut(pointer).ok_or(pointer)? = replacement;
        assert!(to_input(&bad).is_err(), "accepted invalid {pointer}");
    }
    let mut missing = base.clone();
    missing["observations"]["subject"]
        .as_object_mut()
        .ok_or("subject")?
        .remove("muse_version");
    to_input(&missing).err().ok_or("missing version accepted")?;
    let mut unknown = base.clone();
    unknown["success"] = json!(true);
    to_input(&unknown).err().ok_or("outer field accepted")?;
    let mut nested = base;
    nested["observations"]["usage"]["unreported_cost"] = json!(0);
    to_input(&nested).err().ok_or("usage field accepted")?;
    Ok(())
}

#[test]
fn native_muse_accepts_integral_decimal_and_exponent_spellings() -> Result<(), String> {
    let config = config()?;
    let plain = full_fixture()?;
    let expected = parse_ingest(&plain, &config)?;
    let mut encoded = serde_json::to_string(&plain).map_err(|err| err.to_string())?;
    for (from, to) in [
        ("\"input\":100", "\"input\":1e2"),
        ("\"cached_input\":80", "\"cached_input\":80.0"),
        ("\"output\":5", "\"output\":5e0"),
        ("\"total\":105", "\"total\":1.05e2"),
        ("\"tests_executed\":1", "\"tests_executed\":1.0"),
        ("\"exit_code\":0", "\"exit_code\":0.0"),
        ("\"exit_code\":101", "\"exit_code\":1.01e2"),
        ("\"model_calls\":4", "\"model_calls\":4e0"),
        ("\"tool_calls\":8", "\"tool_calls\":8.0"),
    ] {
        assert!(encoded.contains(from), "fixture lacks {from}");
        encoded = encoded.replace(from, to);
    }
    let value = parse_ingest_text(&encoded)?;
    let actual = parse_ingest(&value, &config)?;
    for name in METRICS {
        assert_eq!(
            observed(&actual, name)?,
            observed(&expected, name)?,
            "{name}"
        );
    }
    for (from, to) in [
        ("\"input\":1e2", "\"input\":100.00000000000000001"),
        (
            "\"tests_executed\":1.0",
            "\"tests_executed\":1.00000000000000001",
        ),
        ("\"exit_code\":0.0", "\"exit_code\":0.00000000000000001"),
    ] {
        assert!(encoded.contains(from), "fixture lacks {from}");
        parse_ingest_text(&encoded.replace(from, to))
            .err()
            .ok_or("accepted fractional measurement")?;
    }
    Ok(())
}

#[test]
fn native_muse_schema_validates_original_numbers_before_f64_rounding() -> Result<(), String> {
    let encoded = serde_json::to_string(&full_fixture()?).map_err(|err| err.to_string())?;
    assert!(encoded.contains("\"model_calls\":4"));
    for invalid in ["4.0000000000000000001", "1e400", "-1", "9007199254740992"] {
        let text = encoded.replace("\"model_calls\":4", &format!("\"model_calls\":{invalid}"));
        assert!(parse_ingest_text(&text).is_err(), "accepted {invalid}");
    }
    Ok(())
}

#[test]
fn native_muse_semantics_reject_contradictory_success_and_unbound_evidence() -> Result<(), String> {
    let base = full_fixture()?;
    to_input(&base)?;
    let cases = [
        (
            "/observations/setup/mcp_available",
            json!(false),
            "setup status",
        ),
        (
            "/observations/static/after_evidence",
            json!("setup-log"),
            "static_report",
        ),
        (
            "/observations/durable/current",
            json!(false),
            "ready_to_finish",
        ),
        (
            "/observations/durable/edit_cage",
            json!("violated"),
            "ready_to_finish",
        ),
        (
            "/observations/durable/after_manifest",
            json!("missing"),
            "unknown Muse evidence",
        ),
        (
            "/observations/executions/original/exit_code",
            json!(1),
            "passing execution",
        ),
        (
            "/observations/executions/original/tests_executed",
            json!(0),
            "passing execution",
        ),
        (
            "/observations/executions/counterexample/exit_code",
            json!(0),
            "failed execution",
        ),
        (
            "/observations/executions/counterexample/tests_executed",
            json!(0),
            "assertion failure",
        ),
        (
            "/observations/executions/counterexample/failed_test",
            json!("unrelated_test"),
            "named test selector",
        ),
        (
            "/observations/executions/restored/id",
            json!("original"),
            "distinct IDs",
        ),
        (
            "/observations/executions/restored/tests_sha256",
            json!("0".repeat(64)),
            "same test bytes",
        ),
        (
            "/observations/executions/restored/implementation_sha256",
            json!("0".repeat(64)),
            "original implementation",
        ),
        (
            "/observations/executions/counterexample/implementation_sha256",
            json!("1".repeat(64)),
            "different implementation",
        ),
        (
            "/observations/usage/tokens/cached_input",
            json!(101),
            "cached_input <= input",
        ),
        (
            "/observations/usage/tokens/total",
            json!(104),
            "total = input + output",
        ),
        (
            "/observations/subject/platform",
            json!("   "),
            "blank identity",
        ),
    ];
    for (pointer, replacement, reason) in cases {
        let mut bad = base.clone();
        *bad.pointer_mut(pointer).ok_or(pointer)? = replacement;
        let err = to_input(&bad)
            .err()
            .ok_or_else(|| format!("accepted {pointer}"))?;
        assert!(err.contains(reason), "{pointer}: {err}");
    }
    let mut duplicate = base.clone();
    let first = duplicate["observations"]["evidence"][0].clone();
    duplicate["observations"]["evidence"]
        .as_array_mut()
        .ok_or("evidence")?
        .push(first);
    assert!(
        to_input(&duplicate)
            .err()
            .ok_or("duplicate accepted")?
            .contains("duplicate Muse evidence")
    );
    let mut prose = base;
    prose["observations"]["evidence"][4]["kind"] = json!("tool_transcript");
    assert!(
        to_input(&prose)
            .err()
            .ok_or("prose accepted")?
            .contains("repair_manifest")
    );
    let mut passing_with_failure = full_fixture()?;
    passing_with_failure["observations"]["executions"]["original"]["failed_test"] =
        json!("fixture_boundary");
    assert!(
        to_input(&passing_with_failure)
            .err()
            .ok_or("contradiction accepted")?
            .contains("failed_test")
    );
    Ok(())
}

#[test]
fn native_muse_missing_incomplete_and_negative_observations_stay_distinct() -> Result<(), String> {
    let config = config()?;
    let mut value = full_fixture()?;
    value["observations"]["executions"]
        .as_object_mut()
        .ok_or("executions")?
        .remove("restored");
    let partial = parse_ingest(&value, &config)?;
    assert_eq!(
        observed(&partial, "executed_discrimination")?,
        &SampleOutcome::NotMeasured
    );
    assert_eq!(
        observed(&partial, "durable_ready_reported")?,
        &SampleOutcome::Value(1.0)
    );
    let mut incomplete = full_fixture()?;
    incomplete["observations"]["executions"]["restored"]["status"] = json!("incomplete");
    incomplete["observations"]["usage"]["complete"] = json!(false);
    let samples = parse_ingest(&incomplete, &config)?;
    assert_eq!(
        observed(&samples, "restored_test_passed")?,
        &SampleOutcome::NotMeasured
    );
    assert_eq!(
        observed(&samples, "executed_discrimination")?,
        &SampleOutcome::NotMeasured
    );
    assert_eq!(
        observed(&samples, "model_calls")?,
        &SampleOutcome::Incomplete(4.0)
    );
    let mut compile_failure = full_fixture()?;
    compile_failure["observations"]["executions"]["counterexample"]["failure_kind"] =
        json!("compile");
    compile_failure["observations"]["executions"]["counterexample"]["tests_executed"] = json!(0);
    compile_failure["observations"]["executions"]["counterexample"]
        .as_object_mut()
        .ok_or("leg")?
        .remove("failed_test");
    let samples = parse_ingest(&compile_failure, &config)?;
    assert_eq!(
        observed(&samples, "wrong_control_test_failed")?,
        &SampleOutcome::Value(0.0)
    );
    assert_eq!(
        observed(&samples, "executed_discrimination")?,
        &SampleOutcome::Value(0.0)
    );
    // No-movement static reports may genuinely have identical bytes.
    let mut unchanged = full_fixture()?;
    unchanged["observations"]["static"]["after"] = json!("weakly_exposed");
    unchanged["observations"]["evidence"][2]["sha256"] =
        unchanged["observations"]["evidence"][1]["sha256"].clone();
    assert_eq!(
        observed(&parse_ingest(&unchanged, &config)?, "static_exposed_after")?,
        &SampleOutcome::Value(0.0)
    );
    // Failed attempts may retain the same manifest bytes at a later observation.
    let mut unchanged_attempt = full_fixture()?;
    unchanged_attempt["observations"]["durable"]["after_state"] = json!("failed");
    unchanged_attempt["observations"]["evidence"][4]["sha256"] =
        unchanged_attempt["observations"]["evidence"][3]["sha256"].clone();
    assert_eq!(
        observed(
            &parse_ingest(&unchanged_attempt, &config)?,
            "durable_ready_reported"
        )?,
        &SampleOutcome::Value(0.0)
    );
    Ok(())
}

#[test]
fn native_muse_incomplete_only_keeps_provenance_without_numeric_zero() -> Result<(), String> {
    let mut value = fixture()?;
    value["observations"]["setup"]["status"] = json!("incomplete");
    let config = config()?;
    let samples = parse_ingest(&value, &config)?;
    assert!(
        samples
            .iter()
            .all(|sample| sample.outcome == SampleOutcome::NotMeasured)
    );
    assert!(
        samples
            .iter()
            .any(|sample| sample.detail.contains("synthetic-test-model"))
    );
    let report = build_report(
        &config,
        &["agent".to_string()],
        &samples,
        &RunContext::default(),
        None,
        false,
    );
    assert_eq!(
        report_metric(&report, "setup_passed")?["value"],
        Value::Null
    );
    Ok(())
}

#[test]
fn native_muse_file_admission_rejects_duplicate_distinct_and_generic_runs() -> Result<(), String> {
    let config = config()?;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/verification/ripr/muse-plugin-eval.valid.json");
    let mut seen = BTreeSet::new();
    let samples = load_ingest(&path, &config, &mut seen)?;
    assert!(
        samples
            .iter()
            .any(|sample| sample.detail.contains("input_receipt="))
    );
    assert!(
        load_ingest(&path, &config, &mut seen)
            .err()
            .ok_or("duplicate file accepted")?
            .contains("duplicate Muse plugin record")
    );
    let mut second = fixture()?;
    second["observations"]["run_id"] = json!("different-run");
    assert!(
        register_run(&second, &mut seen)
            .err()
            .ok_or("second run accepted")?
            .contains("only one Muse record")
    );
    let generic = json!({
        "schema_version": super::super::INPUT_SCHEMA_VERSION,
        "source": "muse-plugin",
        "metrics": [{"id": "agent.muse_setup_passed", "value": 1}],
    });
    assert!(register_run(&generic, &mut BTreeSet::new()).is_err());
    assert!(register_run(&generic, &mut seen).is_err());
    let unrelated = json!({
        "schema_version": super::super::INPUT_SCHEMA_VERSION, "source": "first-run"
    });
    register_run(&unrelated, &mut seen)?;
    let sparse = sparse_fixture()?;
    let mut sparse_seen = BTreeSet::new();
    register_run(&sparse, &mut sparse_seen)?;
    assert!(register_run(&sparse, &mut sparse_seen).is_err());
    let mut different_sparse = sparse.clone();
    different_sparse["task"]["initial_request"] = json!("another document");
    assert!(register_run(&different_sparse, &mut sparse_seen).is_err());
    assert!(register_run(&fixture()?, &mut sparse_seen).is_err());
    assert!(register_run(&generic, &mut sparse_seen).is_err());
    Ok(())
}

#[test]
fn native_muse_missing_axes_cannot_bypass_generic_metric_ownership() -> Result<(), String> {
    let mut config = config()?;
    let metric = config
        .metric
        .iter_mut()
        .find(|metric| metric.id == "agent.muse_executed_discrimination")
        .ok_or("registered Muse metric")?;
    metric.source = "measured".to_string();
    for value in [fixture()?, sparse_fixture()?] {
        let err = parse_ingest(&value, &config)
            .err()
            .ok_or("wrong source accepted")?;
        assert!(err.contains("not") && err.contains("ingest:muse-plugin"));
    }
    Ok(())
}

#[test]
fn native_muse_trials_never_claim_baseline_comparability() -> Result<(), String> {
    let config = config()?;
    let first = full_fixture()?;
    let context = RunContext::default();
    let base_samples = parse_ingest(&first, &config)?;
    let baseline = build_report(
        &config,
        &["agent".to_string()],
        &base_samples,
        &context,
        None,
        false,
    );
    for (pointer, replacement) in [
        ("/observations/subject/model", json!("different-model")),
        (
            "/observations/scenario/fixture_sha256",
            json!("0".repeat(64)),
        ),
        ("/observations/usage/model_calls", json!(40)),
        ("/observations/usage/complete", json!(false)),
    ] {
        let mut next = first.clone();
        *next.pointer_mut(pointer).ok_or(pointer)? = replacement;
        if pointer == "/observations/subject/model" {
            next["model"]["model"] = next["observations"]["subject"]["model"].clone();
        }
        if pointer == "/observations/scenario/fixture_sha256" {
            next["identity"]["fixture_digest"] =
                next["observations"]["scenario"]["fixture_sha256"].clone();
        }
        let samples = parse_ingest(&next, &config)?;
        assert_ne!(samples[0].repo, base_samples[0].repo);
        let report = build_report(
            &config,
            &["agent".to_string()],
            &samples,
            &context,
            Some(&baseline),
            true,
        );
        for name in ["setup_passed", "model_calls"] {
            let comparison = &report_metric(&report, name)?["baseline"];
            assert_eq!(comparison["comparable"], false);
            assert_eq!(comparison["regressed"], Value::Null);
            assert!(
                comparison["reason"]
                    .as_str()
                    .ok_or("comparison reason")?
                    .contains("not defined")
            );
        }
    }
    Ok(())
}
