//! Real first-run receipt/export/ingest parity, with literal step-count oracles.

use super::*;
use crate::reports::{first_run_rows_to_input, parse_ingest_text};
use std::sync::atomic::{AtomicUsize, Ordering};

fn metric<'a>(input: &'a Value, id: &str) -> Result<&'a Value, String> {
    input["metrics"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["id"] == id))
        .ok_or_else(|| format!("missing metric {id}"))
}

fn ingest(rows: &[Value]) -> Result<Value, String> {
    let text = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    first_run_rows_to_input(&parse_ingest_text(&text)?)
}

fn exported_walk(
    steps: &[(&str, f64, usize)],
    workflow_lines: Option<usize>,
) -> Result<(Value, Vec<Value>), String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let temp = std::env::temp_dir();
    fs::create_dir_all(&temp).map_err(|err| err.to_string())?;
    let out = temp.join(format!(
        "first-run-budget-ingest-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // Exclusive creation: never reuse or remove another test's directory.
    fs::create_dir(&out).map_err(|err| format!("{}: {err}", out.display()))?;
    let result = (|| {
        let setup = vec![StepResult {
            name: "install_published".to_string(),
            command: "install_published".to_string(),
            exit: Some(0),
            secs: 10.0,
            stdout: String::new(),
            stderr: String::new(),
        }];
        let workflow = workflow_lines.map(|lines| "x\n".repeat(lines));
        let steps = steps
            .iter()
            .map(|(name, secs, lines)| {
                let stdout = if *name == "check" && *lines > 0 {
                    format!("Next\n{}", "x\n".repeat(lines - 1))
                } else {
                    "x\n".repeat(*lines)
                };
                let step = StepResult {
                    name: name.to_string(),
                    command: name.to_string(),
                    exit: Some(0),
                    secs: *secs,
                    stdout,
                    stderr: String::new(),
                };
                let flags = friction(&step, workflow.as_deref());
                (step, flags)
            })
            .collect();
        let case = CaseResult {
            name: "a".to_string(),
            steps,
            verdict: Some("weakly_exposed"),
            verdict_reason: None,
            caught_by: &[],
            workflow_lines,
            workflow_installs_with_cargo: Some(false),
        };
        finish(
            &parse_options(&[])?,
            &out,
            "ripr test",
            setup,
            vec![case],
            None,
        )?;
        let receipt =
            fs::read_to_string(out.join("first-run.json")).map_err(|err| err.to_string())?;
        let receipt = serde_json::from_str(&receipt).map_err(|err| err.to_string())?;
        let text = fs::read_to_string(out.join(ROWS_FILE)).map_err(|err| err.to_string())?;
        let exported = parse_ingest_text(&text)?;
        let rows = exported["rows"]
            .as_array()
            .ok_or("export has no rows")?
            .clone();
        Ok((receipt, rows))
    })();
    fs::remove_dir_all(&out).map_err(|err| format!("{}: {err}", out.display()))?;
    result
}

#[test]
fn exported_dual_budget_breach_counts_once_and_keeps_both_details() -> Result<(), String> {
    let (receipt, rows) = exported_walk(&[("check", 11.0, 61)], None)?;
    assert_eq!(
        receipt["cases"][0]["steps"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(rows.len(), 7);
    let input = ingest(&rows)?;
    let over = metric(&input, "first_run.over_budget_steps")?;
    assert_eq!(over["value"], json!(1));
    assert_eq!(
        over["evidence"],
        json!("a/check secs 11 > 10, a/check stdout_lines 61 > 60")
    );
    assert_eq!(
        crate::public_proof::first_run_value_for_test("first_run.over_budget_steps", &receipt)?,
        Some(1.0)
    );
    assert_eq!(
        metric(&input, "first_run.over_budget_workflows")?["value"],
        json!(0)
    );
    Ok(())
}

#[test]
fn exported_repeated_names_keep_distinct_occurrences_and_workflow_budgets() -> Result<(), String> {
    // Dual breach, output-only breach, time-only breach, workflow-only breach.
    // Boundary-equal values are deliberate: counting only breached rows before
    // assigning occurrence ordinals would collapse the last two check runs.
    let (receipt, rows) = exported_walk(
        &[
            ("check", 11.0, 61),
            ("check", 10.0, 61),
            ("check", 11.0, 60),
            ("init_ci", 5.0, 60),
        ],
        Some(1501),
    )?;
    assert_eq!(
        receipt["cases"][0]["steps"].as_array().map(Vec::len),
        Some(4)
    );
    assert_eq!(rows.len(), 20);
    let input = ingest(&rows)?;
    assert_eq!(
        metric(&input, "first_run.over_budget_steps")?["value"],
        json!(3)
    );
    assert_eq!(
        metric(&input, "first_run.over_budget_steps")?["evidence"],
        json!(
            "a/check secs 11 > 10, a/check stdout_lines 61 > 60, a/check stdout_lines 61 > 60, a/check secs 11 > 10"
        )
    );
    assert_eq!(
        metric(&input, "first_run.over_budget_workflows")?["value"],
        json!(1)
    );
    assert_eq!(
        metric(&input, "first_run.over_budget_workflows")?["evidence"],
        json!("a/init_ci workflow_lines 1501 > 1500")
    );
    for (id, expected) in [
        ("first_run.over_budget_steps", 3.0),
        ("first_run.over_budget_workflows", 1.0),
    ] {
        assert_eq!(
            crate::public_proof::first_run_value_for_test(id, &receipt)?,
            Some(expected)
        );
    }
    assert_eq!(metric(&input, "first_run.failed_steps")?["value"], json!(0));
    assert_eq!(
        metric(&input, "first_run.friction_events")?["value"],
        json!(5.0)
    );
    assert_eq!(metric(&input, "first_run.walk_secs")?["value"], json!(37.0));
    assert_eq!(
        metric(&input, "first_run.time_to_first_useful_result_s")?["value"],
        json!(21.0)
    );

    // Preserve within-metric occurrence order, but move summary/output rows
    // ahead of timings and all exit rows to the end.
    let mut reordered = rows.clone();
    reordered.sort_by_key(|row| match row["metric"].as_str() {
        Some("verdict" | "workflow_lines" | "friction_count") => 0,
        Some("stdout_lines") => 1,
        Some("secs") => 2,
        _ => 3,
    });
    let shuffled = ingest(&reordered)?;
    for id in [
        "first_run.over_budget_steps",
        "first_run.over_budget_workflows",
        "first_run.failed_steps",
        "first_run.walk_secs",
        "first_run.time_to_first_useful_result_s",
    ] {
        assert_eq!(
            metric(&shuffled, id)?["value"],
            metric(&input, id)?["value"],
            "{id}"
        );
    }
    Ok(())
}

#[test]
fn budget_rows_preserve_case_identity_and_incomplete_timing() -> Result<(), String> {
    let row = |case: &str, metric: &str, value: f64, budget: Option<f64>| {
        json!({"schema": ROW_SCHEMA, "ripr": "ripr test", "case": case,
            "step": "check", "metric": metric, "value": value, "budget": budget})
    };
    let rows = vec![
        row("_setup", "secs", 10.0, None),
        // Not an install step: keep the setup route explicit below.
        row("a", "stdout_lines", 61.0, Some(60.0)),
        row("b", "stdout_lines", 61.0, Some(60.0)),
        row("a", "secs", 11.0, Some(10.0)),
        row("a", "exit", 0.0, Some(0.0)),
        row("b", "exit", 0.0, Some(0.0)),
    ];
    let mut rows = rows;
    rows[0]["step"] = json!("install_published");
    let mut install_exit = row("_setup", "exit", 0.0, Some(0.0));
    install_exit["step"] = json!("install_published");
    rows.push(install_exit);
    let input = ingest(&rows)?;
    assert_eq!(
        metric(&input, "first_run.over_budget_steps")?["value"],
        json!(2)
    );
    let metrics = input["metrics"].as_array().ok_or("no metrics")?;
    let walk = metrics
        .iter()
        .find(|r| r["id"] == "first_run.walk_secs" && r["repo"] == "b")
        .ok_or("no b walk")?;
    assert_eq!(walk["completed"], json!(false));
    assert_eq!(walk["evidence"], json!("no secs row for: check"));
    let useful = metrics
        .iter()
        .find(|r| r["id"] == "first_run.time_to_first_useful_result_s" && r["repo"] == "b")
        .ok_or("no b useful result")?;
    assert_eq!(useful["completed"], json!(false));
    rows.retain(|r| !(r["case"] == "a" && r["metric"] == "exit"));
    let cut = ingest(&rows)?;
    assert_eq!(metric(&cut, "first_run.failed_steps")?["value"], json!(1));
    assert_eq!(
        metric(&cut, "first_run.failed_steps")?["evidence"],
        json!("a/check no exit recorded")
    );
    assert_eq!(
        metric(&cut, "first_run.over_budget_steps")?["value"],
        json!(2)
    );
    Ok(())
}
