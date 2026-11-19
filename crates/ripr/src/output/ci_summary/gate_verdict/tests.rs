use super::{GateVerdict, missing_decision_line};
use serde_json::json;

const ACCEPTED: [&str; 4] = [
    "visible-only",
    "acknowledgeable",
    "baseline-check",
    "calibrated-gate",
];

#[test]
fn a_blocked_decision_names_mode_count_and_first_reason() -> Result<(), String> {
    let decision = json!({
        "status": "blocked",
        "mode": "acknowledgeable",
        "summary": {"blocking": 2},
        "decisions": [
            {"decision": "advisory", "gate_reason": "not this one"},
            {"decision": "blocking", "gate_reason": "needs `ripr-waive`", "placement": {"path": "src/a.rs", "line": 9}},
            {"decision": "blocking", "gate_reason": "second"},
        ],
    });
    let verdict = GateVerdict::from_decision(&decision);
    assert_eq!(
        verdict,
        Some(GateVerdict::Blocked {
            mode: "acknowledgeable".to_string(),
            blocking: 2,
            first: Some((
                "needs `ripr-waive`".to_string(),
                Some("src/a.rs:9".to_string())
            )),
            exception: None,
        })
    );
    let verdict = verdict.ok_or("no verdict")?;
    assert_eq!(
        verdict.annotation(),
        "RIPR gate blocked this pull request under acknowledgeable mode: 2 blocking gaps; first: needs `ripr-waive` [src/a.rs:9]. See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact."
    );
    assert_eq!(
        verdict.summary_lines(),
        [
            "### RIPR gate: blocked",
            "**The RIPR gate blocked this pull request** under `acknowledgeable` mode: 2 blocking gaps.",
            "- First blocking reason: `` needs `ripr-waive` `` at `src/a.rs:9`",
            "- Detail: the `Gate decision` section below, and `gate-decision.md` in the `ripr-reports` artifact.",
        ]
    );
    Ok(())
}

/// The stated count and the listed decisions can disagree in a damaged
/// document; the larger is shown so a blocked gate never reads as zero.
#[test]
fn a_blocked_decision_never_reports_fewer_gaps_than_it_lists() -> Result<(), String> {
    let listed_only = json!({
        "status": "blocked",
        "mode": "calibrated-gate",
        "decisions": [{"decision": "blocking", "gate_reason": "only", "placement": {"path": "src/a.rs"}}],
    });
    assert_eq!(
        GateVerdict::from_decision(&listed_only),
        Some(GateVerdict::Blocked {
            mode: "calibrated-gate".to_string(),
            blocking: 1,
            first: Some(("only".to_string(), Some("src/a.rs".to_string()))),
            exception: None,
        })
    );
    let counted_only =
        json!({"status": "blocked", "mode": "calibrated-gate", "summary": {"blocking": 4}});
    let verdict = GateVerdict::from_decision(&counted_only);
    assert_eq!(
        verdict,
        Some(GateVerdict::Blocked {
            mode: "calibrated-gate".to_string(),
            blocking: 4,
            first: None,
            exception: None,
        })
    );
    let verdict = verdict.ok_or("no verdict")?;
    assert_eq!(
        verdict.annotation(),
        "RIPR gate blocked this pull request under calibrated-gate mode: 4 blocking gaps. See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact."
    );
    assert_eq!(verdict.summary_lines().len(), 3);
    Ok(())
}

/// A gate can block on an exception-policy violation with no blocking
/// gap. The verdict then names the violation, not "0 blocking gaps".
#[test]
fn an_exception_policy_block_names_the_violation() -> Result<(), String> {
    let decision = json!({
        "status": "blocked",
        "mode": "acknowledgeable",
        "summary": {"blocking": 0},
        "decisions": [{"decision": "advisory", "gate_reason": "not this one"}],
        "exception_policy": {"violations": [
            {"kind": "note", "detail": "does not stop the gate", "blocking": false},
            {"kind": "expired_exception", "detail": "EX-1 expired 2026-01-01", "blocking": true},
            {"kind": "missing_owner", "detail": "EX-2 has no owner", "blocking": true},
        ]},
    });
    let verdict = GateVerdict::from_decision(&decision).ok_or("no verdict")?;
    assert_eq!(
        verdict.annotation(),
        "RIPR gate blocked this pull request under acknowledgeable mode: 2 blocking exception-policy violations; first: expired_exception: EX-1 expired 2026-01-01. See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact."
    );
    assert_eq!(
        verdict.summary_lines(),
        [
            "### RIPR gate: blocked",
            "**The RIPR gate blocked this pull request** under `acknowledgeable` mode: 2 blocking exception-policy violations.",
            "- First blocking exception-policy violation: `expired_exception: EX-1 expired 2026-01-01`",
            "- Detail: the `Gate decision` section below, and `gate-decision.md` in the `ripr-reports` artifact.",
        ]
    );

    // Gaps and violations together: both are counted, the gap leads.
    let both = json!({
        "status": "blocked",
        "mode": "acknowledgeable",
        "summary": {"blocking": 1},
        "decisions": [{"decision": "blocking", "gate_reason": "no label"}],
        "exception_policy": {"violations": [
            {"kind": "expired_exception", "detail": "EX-1 expired", "blocking": true},
        ]},
    });
    let verdict = GateVerdict::from_decision(&both).ok_or("no verdict")?;
    assert_eq!(
        verdict.annotation(),
        "RIPR gate blocked this pull request under acknowledgeable mode: 1 blocking gap and 1 blocking exception-policy violation; first: no label. See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact."
    );
    Ok(())
}

#[test]
fn a_config_error_carries_its_messages() -> Result<(), String> {
    let decision = json!({
        "status": "config_error",
        "mode": "baseline-check",
        "config_errors": ["one", "two", "three", "four", "five"],
    });
    let verdict = GateVerdict::from_decision(&decision).ok_or("config_error must be a verdict")?;
    assert_eq!(
        verdict.annotation(),
        "RIPR gate could not evaluate this pull request under baseline-check mode (configuration error, not a finding about the code): one"
    );
    assert_eq!(
        verdict.summary_lines(),
        [
            "### RIPR gate: configuration error",
            "**The RIPR gate could not evaluate this pull request** under `baseline-check` mode. This is a setup problem, not a finding about the code.",
            "- Config error: `one`",
            "- Config error: `two`",
            "- Config error: `three`",
            "- 2 more in `gate-decision.md` in the `ripr-reports` artifact.",
        ]
    );
    let bare =
        GateVerdict::from_decision(&json!({"status": "config_error", "mode": "baseline-check"}));
    assert_eq!(
        bare.map(|verdict| verdict.summary_lines().len()),
        Some(3),
        "a config_error without messages still points at the report"
    );
    Ok(())
}

/// Only the two statuses this module words are verdicts. A passing
/// status, and a status this module does not know, print nothing.
#[test]
fn other_statuses_are_not_verdicts() {
    for status in ["pass", "advisory", "acknowledged", "fail", "", "BLOCKED"] {
        let decision =
            json!({"status": status, "mode": "acknowledgeable", "summary": {"blocking": 3}});
        assert_eq!(GateVerdict::from_decision(&decision), None, "{status:?}");
    }
    assert_eq!(
        GateVerdict::from_decision(&json!({"mode": "acknowledgeable"})),
        None
    );
    assert_eq!(GateVerdict::from_decision(&json!([])), None);
}

#[test]
fn only_a_set_mode_the_gate_refuses_is_a_verdict() -> Result<(), String> {
    assert_eq!(GateVerdict::from_mode(""), None);
    for mode in ACCEPTED {
        assert_eq!(GateVerdict::from_mode(mode), None, "{mode}");
    }
    for mode in ["visible_only", "Visible-Only", " visible-only", "off", "  "] {
        let verdict = GateVerdict::from_mode(mode).ok_or(format!("{mode:?} must be refused"))?;
        let annotation = verdict.annotation();
        assert!(
            annotation.starts_with("RIPR gate did not run: unknown gate mode "),
            "{annotation}"
        );
        assert!(annotation.contains(&format!("{mode:?}")), "{annotation}");
        assert!(
            annotation.contains("RIPR_GATE_MODE repository variable"),
            "{annotation}"
        );
        assert!(!annotation.contains('\n'), "{annotation}");
    }
    assert_eq!(
        GateVerdict::from_mode("visible_only").map(|verdict| verdict.summary_lines()),
        Some(vec![
            "### RIPR gate: unknown `RIPR_GATE_MODE`".to_string(),
            "**The RIPR gate did not run**, because `RIPR_GATE_MODE` is not a gate mode. This is a setup problem, not a finding about the code.".to_string(),
            "- Gate message: ``unknown gate mode \"visible_only\". Did you mean `visible-only`? Accepted: visible-only, acknowledgeable, baseline-check, calibrated-gate.``".to_string(),
            "- Next step: fix the `RIPR_GATE_MODE` repository variable (Settings > Secrets and variables > Actions > Variables), or unset it to stay advisory, then re-run the job.".to_string(),
        ])
    );
    Ok(())
}

#[test]
fn the_missing_decision_line_follows_the_mode() {
    assert_eq!(
        missing_decision_line(""),
        "Gate decision was not run. Set `RIPR_GATE_MODE` to `visible-only`, `acknowledgeable`, `baseline-check`, or `calibrated-gate` to opt in."
    );
    assert_eq!(
        missing_decision_line("visible_only"),
        "Gate decision was not run: `RIPR_GATE_MODE` is set to `visible_only`, which is not a gate mode. Set it to `visible-only`, `acknowledgeable`, `baseline-check`, or `calibrated-gate`, or unset it to stay advisory."
    );
    for mode in ACCEPTED {
        let line = missing_decision_line(mode);
        assert!(
            line.starts_with(&format!(
                "Gate decision was not written although `RIPR_GATE_MODE` is `{mode}`: "
            )),
            "{line}"
        );
        assert!(!line.contains("opt in"), "{line}");
    }
}
