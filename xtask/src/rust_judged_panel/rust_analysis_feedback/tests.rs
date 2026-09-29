//! Discriminating controls for the Rust analysis-feedback ledger (#4796).

use serde_json::{Value, json};

use super::lifecycle;
use super::owners;
use super::report;
use super::schema::{
    self, FeedbackRow, JudgmentFact, derived_failure_direction, is_analyzer_defect,
};
use super::{FeedbackLedger, sha256_bytes, validate_bundle_for_test};

const JUDGMENTS_SHA: &str = "sha256:test-judgments";

fn fact(
    case_id: &str,
    terminal: &str,
    false_actionable: Option<bool>,
    false_exposed: Option<bool>,
    under_credit: Option<bool>,
    limitation_correct: Option<bool>,
) -> JudgmentFact {
    JudgmentFact {
        case_id: case_id.into(),
        expected_direction: "should_gap".into(),
        terminal: terminal.into(),
        false_actionable,
        false_exposed,
        under_credit,
        limitation_correct,
    }
}

fn row_json(case_id: &str, direction: &str, status: &str, reduction: &str) -> Value {
    json!({
        "feedback_id": format!("fb-{case_id}"),
        "case_id": case_id,
        "source": {
            "panel": "rust-judged-behavior-panel",
            "judgments": "EffortlessMetrics/ripr-swarm#3806",
            "calibration": "EffortlessMetrics/ripr-swarm#4795"
        },
        "behavior_identity": format!("{case_id}:owner"),
        "terminal_judgment": "confirmed_should_gap",
        "runtime_calibration": { "status": "not_run", "result": null },
        "failure_direction": direction,
        "mechanism": format!("mechanism-{case_id}"),
        "analyzer_family": "rust-static",
        "semantic_owner": "crates/ripr/src/analysis",
        "reduction": {
            "disposition": reduction,
            "fixture_id": null,
            "expected_class": null,
            "reason": "synthetic control",
            "materialization_boundary": if reduction == "replay_only" || reduction == "unreduced" {
                Value::String("historical_exact_commit".into())
            } else {
                Value::Null
            },
            "positive_control": null,
            "negative_control": null
        },
        "target_identity": if direction == "wrong_target" {
            Value::String("exact::observer".into())
        } else {
            Value::Null
        },
        "owner": {
            "existing": null,
            "designated": if schema::ANALYZER_DEFECTS.contains(&direction) {
                "unowned_no_github_mutation"
            } else {
                "none"
            },
            "search_receipt": "all-state search recorded in the production ledger",
            "competing": []
        },
        "status": status,
        "repair": {
            "merged_implementation": null,
            "original_case_replay": false,
            "replay_identities": null
        },
        "non_claims": ["synthetic"],
        "notes": null
    })
}

fn ledger(rows: Vec<Value>) -> Value {
    json!({
        "schema_version": "0.1",
        "kind": "rust_judged_panel_feedback_ledger",
        "authority": "EffortlessMetrics/ripr-swarm#4796",
        "inherited_authorities": ["EffortlessMetrics/ripr-swarm#3164", "EffortlessMetrics/ripr-swarm#3806"],
        "excluded_authorities": ["EffortlessMetrics/ripr-swarm#4795"],
        "release_judgments_path": "metrics/rust-judged-behavior-panel/release-judgments.json",
        "release_judgments_sha256": JUDGMENTS_SHA,
        "calibration": {
            "status": "not_run",
            "authority": "EffortlessMetrics/ripr-swarm#4795",
            "reason": "Calibration producer has not landed."
        },
        "limits": ["synthetic"],
        "rows": rows
    })
}

fn parse_row(value: Value) -> Result<FeedbackRow, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn parse_ledger(value: Value) -> Result<FeedbackLedger, String> {
    FeedbackLedger::from_json(value)
}

fn repo_root() -> Result<&'static std::path::Path, String> {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest has no repository parent".to_string())
}

#[test]
fn rust_analysis_feedback_false_exposed_is_not_false_actionable() {
    let exposed = fact(
        "fe",
        "confirmed_should_gap",
        None,
        Some(true),
        Some(false),
        None,
    );
    let actionable = fact(
        "fa",
        "confirmed_should_stay_quiet",
        Some(true),
        Some(false),
        None,
        None,
    );
    assert_eq!(
        derived_failure_direction(&exposed, "false_exposed"),
        "false_exposed"
    );
    assert_eq!(
        derived_failure_direction(&actionable, "false_actionable"),
        "false_actionable"
    );
    assert_ne!(
        derived_failure_direction(&exposed, "false_actionable"),
        "false_actionable"
    );
}

#[test]
fn rust_analysis_feedback_under_credit_is_not_support_promotion() {
    let fact = fact(
        "uc",
        "confirmed_should_gap",
        None,
        Some(false),
        Some(true),
        None,
    );
    assert_eq!(
        derived_failure_direction(&fact, "static_under_credit"),
        "static_under_credit"
    );
    assert!(!matches!(
        derived_failure_direction(&fact, "static_under_credit"),
        "no_confirmed_failure"
    ));
}

#[test]
fn rust_analysis_feedback_wrong_target_requires_exact_identity() -> Result<(), String> {
    let fact = fact(
        "wt",
        "confirmed_should_stay_quiet",
        Some(false),
        Some(false),
        Some(false),
        None,
    );
    assert_eq!(
        derived_failure_direction(&fact, "wrong_target"),
        "wrong_target"
    );
    let mut row = row_json("wt", "wrong_target", "open", "replay_only");
    row["terminal_judgment"] = json!("confirmed_should_stay_quiet");
    row["target_identity"] = Value::Null;
    let violations = schema::validate_row(&parse_row(row)?, &fact);
    if violations
        .iter()
        .any(|item| item.contains("target_identity"))
    {
        Ok(())
    } else {
        Err(format!("missing target_identity violation: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_wrong_target_cannot_close_on_a_nearby_target() -> Result<(), String> {
    let mut row = row_json("wt", "wrong_target", "closed_with_replay", "replay_only");
    row["terminal_judgment"] = json!("confirmed_should_stay_quiet");
    row["repair"] = json!({
        "merged_implementation": "abc123",
        "original_case_replay": true,
        "replay_identities": "nearby::other_observer"
    });
    let violations = lifecycle::validate_row(&parse_row(row)?);
    if violations.iter().any(|item| item.contains("exact target")) {
        Ok(())
    } else {
        Err(format!("nearby target was accepted: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_correct_limitation_is_not_an_analyzer_defect() {
    let fact = fact(
        "lim",
        "confirmed_should_limit",
        None,
        Some(false),
        None,
        Some(true),
    );
    assert_eq!(
        derived_failure_direction(&fact, "no_confirmed_failure"),
        "no_confirmed_failure"
    );
    assert!(!is_analyzer_defect("no_confirmed_failure"));
    assert!(!is_analyzer_defect("inconclusive_no_feedback"));
}

#[test]
fn rust_analysis_feedback_incorrect_limitation_cannot_close_as_accepted() -> Result<(), String> {
    let fact = fact(
        "lim",
        "confirmed_should_limit",
        None,
        Some(false),
        None,
        Some(false),
    );
    let mut row = row_json(
        "lim",
        "incorrect_limitation",
        "accepted_limitation",
        "replay_only",
    );
    row["terminal_judgment"] = json!("confirmed_should_limit");
    let violations = schema::validate_row(&parse_row(row)?, &fact);
    if violations
        .iter()
        .any(|item| item.contains("accepted_limitation"))
    {
        Ok(())
    } else {
        Err(format!(
            "incorrect limitation closed as accepted: {violations:?}"
        ))
    }
}

#[test]
fn rust_analysis_feedback_duplicate_owner_is_detected() -> Result<(), String> {
    let mut a = row_json("a", "false_exposed", "open", "replay_only");
    let mut b = row_json("b", "false_exposed", "open", "replay_only");
    a["mechanism"] = json!("same-mechanism");
    b["mechanism"] = json!("same-mechanism");
    a["owner"]["designated"] = json!("issue:111");
    b["owner"]["designated"] = json!("issue:222");
    let violations = owners::validate_duplicates(&[parse_row(a)?, parse_row(b)?]);
    if violations
        .iter()
        .any(|item| item.contains("rival designated owners"))
    {
        Ok(())
    } else {
        Err(format!("rival owners were accepted: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_existing_owner_cannot_be_left_unowned() -> Result<(), String> {
    let mut row = row_json("fe", "false_exposed", "open", "replay_only");
    row["owner"]["existing"] = json!("issue:111");
    row["owner"]["designated"] = json!("unowned_no_github_mutation");
    let violations = owners::validate_row(&parse_row(row)?);
    if violations
        .iter()
        .any(|item| item.contains("existing owner must be reused"))
    {
        Ok(())
    } else {
        Err(format!("existing owner was left unowned: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_fixture_hardcoded_to_case_id_is_rejected() -> Result<(), String> {
    let fact = fact(
        "case-x",
        "confirmed_should_gap",
        None,
        Some(true),
        Some(false),
        None,
    );
    let mut row = row_json("case-x", "false_exposed", "open", "fixture_backed");
    row["reduction"]["fixture_id"] = json!("case-x");
    row["reduction"]["positive_control"] = json!("pos");
    row["reduction"]["negative_control"] = json!("neg");
    let violations = schema::validate_row(&parse_row(row)?, &fact);
    if violations
        .iter()
        .any(|item| item.contains("hard-coded to case/feedback id"))
    {
        Ok(())
    } else {
        Err(format!(
            "hard-coded fixture id was accepted: {violations:?}"
        ))
    }
}

#[test]
fn rust_analysis_feedback_repaired_fixture_without_original_replay_stays_pending()
-> Result<(), String> {
    let mut row = row_json(
        "fe",
        "false_exposed",
        "closed_with_replay",
        "fixture_backed",
    );
    row["reduction"]["fixture_id"] = json!("honest-fixture");
    row["reduction"]["positive_control"] = json!("pos");
    row["reduction"]["negative_control"] = json!("neg");
    row["reduction"]["materialization_boundary"] = Value::Null;
    row["repair"] = json!({
        "merged_implementation": "deadbeef",
        "original_case_replay": false,
        "replay_identities": null
    });
    let violations = lifecycle::validate_row(&parse_row(row)?);
    if violations
        .iter()
        .any(|item| item.contains("repaired_pending_replay"))
    {
        Ok(())
    } else {
        Err(format!("fixture pass closed the row: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_stale_judgment_digest_invalidates_closure() -> Result<(), String> {
    let fact = fact(
        "fe",
        "confirmed_should_gap",
        None,
        Some(true),
        Some(false),
        None,
    );
    let mut body = ledger(vec![row_json("fe", "false_exposed", "open", "replay_only")]);
    body["release_judgments_sha256"] = json!("sha256:stale");
    let parsed = parse_ledger(body)?;
    let violations = validate_bundle_for_test(&parsed, &[fact], JUDGMENTS_SHA);
    if violations.iter().any(|item| item.contains("stale closure")) {
        Ok(())
    } else {
        Err(format!("stale digest was accepted: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_reordered_rows_make_byte_stable_reports() -> Result<(), String> {
    let a = parse_row(row_json("a-row", "false_exposed", "open", "replay_only"))?;
    let b = parse_row(row_json("b-row", "false_exposed", "open", "replay_only"))?;
    let first = report::render_from_rows_for_test(vec![a.clone(), b.clone()], 2, JUDGMENTS_SHA)?;
    let second = report::render_from_rows_for_test(vec![b, a], 2, JUDGMENTS_SHA)?;
    if first.json == second.json
        && first.markdown == second.markdown
        && sha256_bytes(first.json.as_bytes()) == sha256_bytes(second.json.as_bytes())
    {
        Ok(())
    } else {
        Err("reordered rows changed report bytes".into())
    }
}

#[test]
fn rust_analysis_feedback_deleting_an_unfavorable_row_changes_the_denominator() -> Result<(), String>
{
    let fe = parse_row(row_json("fe", "false_exposed", "open", "replay_only"))?;
    let quiet = {
        let mut value = row_json(
            "quiet",
            "no_confirmed_failure",
            "no_repair_required",
            "not_a_defect",
        );
        value["terminal_judgment"] = json!("confirmed_should_stay_quiet");
        value["owner"]["designated"] = json!("none");
        parse_row(value)?
    };
    let full =
        report::render_from_rows_for_test(vec![fe.clone(), quiet.clone()], 2, JUDGMENTS_SHA)?;
    let deleted = report::render_from_rows_for_test(vec![quiet], 1, JUDGMENTS_SHA)?;
    if full.json == deleted.json {
        return Err("deleting an unfavorable row left the report unchanged".into());
    }
    if full.analyzer_defect_count != 1 || deleted.analyzer_defect_count != 0 {
        return Err(format!(
            "defect counts did not move: full={} deleted={}",
            full.analyzer_defect_count, deleted.analyzer_defect_count
        ));
    }
    if full.json.contains("\"rows\": 2") && deleted.json.contains("\"rows\": 1") {
        Ok(())
    } else {
        Err("row counts did not move with the denominator".into())
    }
}

#[test]
fn rust_analysis_feedback_human_notes_cannot_strengthen_inconclusive_or_limitation()
-> Result<(), String> {
    let fact = fact(
        "inc",
        "inconclusive_missing_evidence",
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        derived_failure_direction(&fact, "false_exposed"),
        "inconclusive_no_feedback"
    );
    let mut row = row_json(
        "inc",
        "inconclusive_no_feedback",
        "no_repair_required",
        "not_a_defect",
    );
    row["terminal_judgment"] = json!("inconclusive_missing_evidence");
    row["notes"] = json!("this is obviously false_exposed and should be repaired");
    row["owner"]["designated"] = json!("none");
    let violations = schema::validate_row(&parse_row(row.clone())?, &fact);
    if !violations.is_empty() {
        return Err(format!("notes must be ignored: {violations:?}"));
    }
    row["failure_direction"] = json!("false_exposed");
    let strengthened = schema::validate_row(&parse_row(row)?, &fact);
    if strengthened
        .iter()
        .any(|item| item.contains("notes cannot strengthen"))
    {
        Ok(())
    } else {
        Err(format!("notes strengthened the class: {strengthened:?}"))
    }
}

#[test]
fn rust_analysis_feedback_every_judged_case_needs_a_row() -> Result<(), String> {
    let facts = vec![
        fact(
            "a",
            "confirmed_should_gap",
            None,
            Some(true),
            Some(false),
            None,
        ),
        fact(
            "b",
            "confirmed_should_stay_quiet",
            Some(false),
            Some(false),
            None,
            None,
        ),
    ];
    let parsed = parse_ledger(ledger(vec![row_json(
        "a",
        "false_exposed",
        "open",
        "replay_only",
    )]))?;
    let violations = validate_bundle_for_test(&parsed, &facts, JUDGMENTS_SHA);
    if violations
        .iter()
        .any(|item| item.contains("b:") && item.contains("no feedback disposition"))
    {
        Ok(())
    } else {
        Err(format!("missing judged case was accepted: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_extra_row_is_not_a_judged_case() -> Result<(), String> {
    let facts = vec![fact(
        "a",
        "confirmed_should_gap",
        None,
        Some(true),
        Some(false),
        None,
    )];
    let parsed = parse_ledger(ledger(vec![
        row_json("a", "false_exposed", "open", "replay_only"),
        {
            let mut extra = row_json("ghost", "false_exposed", "open", "replay_only");
            extra["terminal_judgment"] = json!("confirmed_should_gap");
            extra
        },
    ]))?;
    let violations = validate_bundle_for_test(&parsed, &facts, JUDGMENTS_SHA);
    if violations
        .iter()
        .any(|item| item.contains("ghost:") && item.contains("not a terminal judged case"))
    {
        Ok(())
    } else {
        Err(format!("extra row was accepted: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_calibration_cannot_set_static_class() -> Result<(), String> {
    let fact = fact(
        "fe",
        "confirmed_should_gap",
        None,
        Some(true),
        Some(false),
        None,
    );
    let mut row = row_json("fe", "false_exposed", "open", "replay_only");
    row["runtime_calibration"] = json!({ "status": "caught", "result": "caught" });
    let violations = schema::validate_row(&parse_row(row)?, &fact);
    if violations.iter().any(|item| item.contains("#4795")) {
        Ok(())
    } else {
        Err(format!("runtime calibration set the class: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_recorded_calibration_belongs_to_4795() -> Result<(), String> {
    let fact = fact(
        "fe",
        "confirmed_should_gap",
        None,
        Some(true),
        Some(false),
        None,
    );
    let mut body = ledger(vec![row_json("fe", "false_exposed", "open", "replay_only")]);
    body["calibration"]["status"] = json!("recorded");
    let parsed = parse_ledger(body)?;
    let violations = validate_bundle_for_test(&parsed, &[fact], JUDGMENTS_SHA);
    if violations.iter().any(|item| item.contains("#4795")) {
        Ok(())
    } else {
        Err(format!("recorded calibration was accepted: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_expected_class_shortcut_is_rejected() -> Result<(), String> {
    let fact = fact(
        "fe",
        "confirmed_should_gap",
        None,
        Some(true),
        Some(false),
        None,
    );
    let mut row = row_json("fe", "false_exposed", "open", "replay_only");
    row["reduction"]["expected_class"] = json!("exposed");
    let violations = schema::validate_row(&parse_row(row)?, &fact);
    if violations
        .iter()
        .any(|item| item.contains("expected_class shortcut"))
    {
        Ok(())
    } else {
        Err(format!(
            "expected_class shortcut was accepted: {violations:?}"
        ))
    }
}

#[test]
fn rust_analysis_feedback_non_defect_cannot_enter_repair_lifecycle() -> Result<(), String> {
    let fact = fact(
        "quiet",
        "confirmed_should_stay_quiet",
        Some(false),
        Some(false),
        None,
        None,
    );
    let mut row = row_json("quiet", "no_confirmed_failure", "open", "not_a_defect");
    row["terminal_judgment"] = json!("confirmed_should_stay_quiet");
    row["owner"]["designated"] = json!("none");
    let violations = schema::validate_row(&parse_row(row)?, &fact);
    if violations
        .iter()
        .any(|item| item.contains("non-defect row cannot enter the repair lifecycle"))
    {
        Ok(())
    } else {
        Err(format!(
            "non-defect entered repair lifecycle: {violations:?}"
        ))
    }
}

#[test]
fn rust_analysis_feedback_json_and_markdown_agree_on_counts() -> Result<(), String> {
    let row = parse_row(row_json("fe", "false_exposed", "open", "replay_only"))?;
    let rendered = report::render_from_rows_for_test(vec![row], 1, JUDGMENTS_SHA)?;
    let json: Value = serde_json::from_str(&rendered.json).map_err(|error| error.to_string())?;
    if json["counts"]["rows"] != 1 || json["counts"]["analyzer_defects"] != 1 {
        return Err("JSON counts drifted from the rendered rows".into());
    }
    if json["counts"]["by_analyzer_family"]["rust-static"] != 1 {
        return Err("family count was omitted from the DTO".into());
    }
    if !rendered.markdown.contains("Rows: 1")
        || !rendered.markdown.contains("Analyzer defects: 1")
        || !rendered.markdown.contains("`rust-static`: 1")
    {
        return Err("Markdown counts drifted from JSON".into());
    }
    if rendered.json.contains("overall_score") || rendered.markdown.contains("accuracy") {
        return Err("report claimed an overall analyzer score".into());
    }
    Ok(())
}

#[test]
fn rust_analysis_feedback_production_ledger_covers_every_judged_case() -> Result<(), String> {
    super::validate_at(repo_root()?)
}

#[test]
fn rust_analysis_feedback_production_reports_carry_the_judged_denominator() -> Result<(), String> {
    let bundle = super::load_and_validate(repo_root()?)?;
    let rendered = report::render(&bundle)?;
    let json: Value = serde_json::from_str(&rendered.json).map_err(|error| error.to_string())?;
    if json["counts"]["judged_cases"] != rendered.row_count {
        return Err("production report judged_cases drifted from rows".into());
    }
    if rendered.row_count == 0 {
        return Err("production report had zero rows".into());
    }
    if rendered.json.contains("overall_score") {
        return Err("production report claimed an overall analyzer score".into());
    }
    Ok(())
}

#[test]
fn rust_analysis_feedback_staging_is_deterministic_and_checked() -> Result<(), String> {
    let row = parse_row(row_json("fe", "false_exposed", "open", "replay_only"))?;
    let rendered = report::render_from_rows_for_test(vec![row], 1, JUDGMENTS_SHA)?;
    let dir = std::env::temp_dir().join(format!(
        "ripr-rust-feedback-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    super::write_staging_for_test(&dir, &rendered)?;
    super::verify_staged_for_test(&dir, &rendered)?;
    let mut drifted = rendered.json.clone();
    drifted.push(' ');
    let drifted_report = super::report::RenderedFeedback {
        json: drifted,
        markdown: rendered.markdown.clone(),
        row_count: 1,
        analyzer_defect_count: 1,
    };
    let error = super::verify_staged_for_test(&dir, &drifted_report)
        .err()
        .ok_or_else(|| "drift must fail --check".to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    if error.contains("does not match a fresh derivation") {
        Ok(())
    } else {
        Err(format!("unexpected staging error: {error}"))
    }
}

#[test]
fn rust_analysis_feedback_merged_repair_without_replay_cannot_stay_open() -> Result<(), String> {
    let mut row = row_json("fe", "false_exposed", "open", "replay_only");
    row["repair"] = json!({
        "merged_implementation": "abc",
        "original_case_replay": false,
        "replay_identities": null
    });
    let violations = lifecycle::validate_row(&parse_row(row)?);
    if violations
        .iter()
        .any(|item| item.contains("repaired_pending_replay"))
    {
        Ok(())
    } else {
        Err(format!("merged repair stayed open: {violations:?}"))
    }
}

#[test]
fn rust_analysis_feedback_unknown_cli_flag_is_rejected() -> Result<(), String> {
    let error = super::parse_feedback_args_for_test(&["--score".into()])
        .err()
        .ok_or_else(|| "unknown flag was accepted".to_string())?;
    if error.contains("unknown rust-judged-panel feedback argument `--score`") {
        Ok(())
    } else {
        Err(format!("unexpected CLI error: {error}"))
    }
}

#[test]
fn rust_analysis_feedback_out_and_check_parse_together() -> Result<(), String> {
    let (out_dir, check) =
        super::parse_feedback_args_for_test(&["--out".into(), "tmp/out".into(), "--check".into()])?;
    if out_dir == "tmp/out" && check {
        Ok(())
    } else {
        Err(format!("parsed out_dir={out_dir} check={check}"))
    }
}
