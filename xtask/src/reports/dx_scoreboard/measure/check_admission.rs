//! Local check-sample admission for the DX scoreboard.
//!
//! A successful speed sample must mean the intended analysis completed for
//! the intended subject. `ripr`'s `analysis_outcome` types and the
//! review-comments validator already own that contract, but they are
//! `pub(crate)` inside the product crate. Promoting them just so xtask can
//! call them would widen the public API; a general schema-validator crate
//! would exceed #7259. This module therefore applies a bounded local
//! predicate against the published check-JSON envelope and the fields the
//! producer already documents.
//!
//! Analysis completeness and presentation completeness stay separate: a
//! findings-array rendering cap does not by itself make the analysis
//! incomplete, but a trust scan that needs omitted findings cannot appear
//! clean.

use serde_json::Value;

/// Current `ripr check --format json` envelope version.
pub(crate) const CHECK_SCHEMA_VERSION: &str = "0.2";
/// Versioned analysis-outcome DTO carried inside the envelope.
pub(crate) const OUTCOME_SCHEMA_VERSION: &str = "0.1";
pub(crate) const PRODUCER_TOOL: &str = "ripr";
/// Copied from the producer claim-boundary constant so a random object
/// with the right keys is not admitted. Keep in lockstep with
/// `crates/ripr/src/analysis_outcome.rs`.
pub(crate) const OUTCOME_CLAIM_BOUNDARY: &str = "Static analysis outcome only; no correctness, test-adequacy, runtime-execution, or merge-readiness claim.";
/// `run_limitations[].run_status` when the findings array is a prefix.
pub(crate) const FINDINGS_BOUND_STATUS: &str = "limited_findings_bound";

const COMPLETE_KINDS: [&str; 5] = [
    "no_scope",
    "no_changed_lines",
    "no_behavioral_candidates",
    "complete_no_findings",
    "complete_with_findings",
];
const INCOMPLETE_KINDS: [&str; 3] = [
    "partial_with_limitations",
    "unsupported_input",
    "analysis_failed",
];

/// Identity the measured child was asked to analyze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckSubject {
    pub(crate) root: String,
    pub(crate) mode: String,
    pub(crate) base: String,
    /// Checkout HEAD when known. Absent means the field is unknown, not
    /// that any head is acceptable.
    pub(crate) head: Option<String>,
}

/// Whether a captured check child may enter the successful complete-work
/// population.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CheckAdmission {
    Complete {
        findings: u64,
        rendering_truncated: bool,
        kind: String,
    },
    Rejected {
        reason: String,
    },
}

impl CheckAdmission {
    pub(crate) fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }

    pub(crate) fn reason(&self) -> &str {
        match self {
            Self::Complete { kind, .. } => kind,
            Self::Rejected { reason } => reason,
        }
    }

    pub(crate) fn findings(&self) -> Option<u64> {
        match self {
            Self::Complete { findings, .. } => Some(*findings),
            Self::Rejected { .. } => None,
        }
    }

    pub(crate) fn rendering_truncated(&self) -> bool {
        matches!(
            self,
            Self::Complete {
                rendering_truncated: true,
                ..
            }
        )
    }
}

/// Admit a captured check child. Validation is intentionally outside the
/// measured interval: callers pass already-captured stdout.
pub(crate) fn admit_check_sample(
    timed_out: bool,
    exited_zero: bool,
    stdout: &str,
    subject: &CheckSubject,
) -> CheckAdmission {
    if timed_out {
        return CheckAdmission::Rejected {
            reason: "harness deadline reached".to_string(),
        };
    }
    if !exited_zero {
        return CheckAdmission::Rejected {
            reason: "child did not exit zero".to_string(),
        };
    }
    let value = match serde_json::from_str::<Value>(stdout) {
        Ok(value) => value,
        Err(_) => {
            return CheckAdmission::Rejected {
                reason: "stdout is not JSON".to_string(),
            };
        }
    };
    admit_check_document(&value, subject)
}

/// The pre-#7259 predicate: any exit-zero child whose stdout parses as JSON,
/// including `{}` and `null`. Kept so tests can prove the new gate rejects
/// documents the old one would accept.
pub(crate) fn permissive_json_exit_zero(timed_out: bool, exited_zero: bool, stdout: &str) -> bool {
    !timed_out && exited_zero && serde_json::from_str::<Value>(stdout).is_ok()
}

pub(crate) fn admit_check_document(value: &Value, subject: &CheckSubject) -> CheckAdmission {
    if value.is_null() {
        return rejected("stdout JSON is null");
    }
    let Some(object) = value.as_object() else {
        return rejected("stdout JSON is not an object");
    };
    match object.get("schema_version").and_then(Value::as_str) {
        Some(CHECK_SCHEMA_VERSION) => {}
        Some(other) => return rejected(&format!("unsupported schema_version `{other}`")),
        None => return rejected("missing schema_version"),
    }
    match object.get("tool").and_then(Value::as_str) {
        Some(PRODUCER_TOOL) => {}
        Some(other) => return rejected(&format!("producer tool must be ripr, got `{other}`")),
        None => return rejected("missing producer tool"),
    }
    for (field, expected) in [
        ("mode", subject.mode.as_str()),
        ("root", subject.root.as_str()),
        ("base", subject.base.as_str()),
    ] {
        match object.get(field).and_then(Value::as_str) {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                return rejected(&format!(
                    "producer {field} `{actual}` does not match expected `{expected}`"
                ));
            }
            None => return rejected(&format!("missing string field {field}")),
        }
    }
    if let Some(expected_head) = subject.head.as_deref()
        && let Some(actual) = object
            .get("head")
            .and_then(Value::as_object)
            .and_then(|head| head.get("commit"))
            .and_then(Value::as_str)
        && actual != expected_head
    {
        return rejected(&format!(
            "producer head `{actual}` does not match expected `{expected_head}`"
        ));
    }
    if !object.get("summary").is_some_and(Value::is_object) {
        return rejected("missing summary object");
    }
    if !object.get("findings").is_some_and(Value::is_array) {
        return rejected("missing findings array");
    }
    let Some(envelope) = object.get("analysis_outcome") else {
        return rejected("missing analysis_outcome");
    };
    if envelope.is_null() {
        return rejected("analysis_outcome is null");
    }
    let Some(envelope) = envelope.as_object() else {
        return rejected("analysis_outcome is not an object");
    };
    let Some(analysis_complete) = envelope.get("analysis_complete").and_then(Value::as_bool) else {
        return rejected("analysis_complete is missing or not boolean");
    };
    let Some(outcome) = envelope.get("outcome").and_then(Value::as_object) else {
        return rejected("analysis_outcome.outcome is missing");
    };
    match outcome.get("schema_version").and_then(Value::as_str) {
        Some(OUTCOME_SCHEMA_VERSION) => {}
        Some(other) => {
            return rejected(&format!(
                "unsupported analysis outcome schema_version `{other}`"
            ));
        }
        None => return rejected("missing analysis outcome schema_version"),
    }
    match outcome.get("claim_boundary").and_then(Value::as_str) {
        Some(OUTCOME_CLAIM_BOUNDARY) => {}
        Some(_) => return rejected("analysis outcome claim_boundary does not match the contract"),
        None => return rejected("missing analysis outcome claim_boundary"),
    }
    let Some(kind) = outcome.get("kind").and_then(Value::as_str) else {
        return rejected("missing analysis outcome kind");
    };
    let kind_complete = COMPLETE_KINDS.contains(&kind);
    let kind_known = kind_complete || INCOMPLETE_KINDS.contains(&kind);
    if !kind_known {
        return rejected(&format!("unknown analysis outcome kind `{kind}`"));
    }
    if analysis_complete != kind_complete {
        return rejected("analysis_complete does not match typed outcome kind");
    }
    if let Some(actual) = outcome
        .get("identity")
        .and_then(Value::as_object)
        .and_then(|identity| identity.get("base_revision"))
        .and_then(Value::as_str)
        && actual != subject.base
    {
        return rejected(&format!(
            "typed outcome base_revision `{actual}` does not match expected `{}`",
            subject.base
        ));
    }
    if !analysis_complete {
        return rejected(&format!("analysis is not complete (`{kind}`)"));
    }
    let findings = finding_count(object, outcome);
    if kind == "complete_with_findings" && findings.unwrap_or(0) == 0 {
        return rejected("complete_with_findings requires a findings count");
    }
    CheckAdmission::Complete {
        findings: findings.unwrap_or(0),
        rendering_truncated: findings_rendering_truncated(value),
        kind: kind.to_string(),
    }
}

/// Scan contradictions only over a fully inspectable findings population.
/// A missing array or a declared rendering prefix cannot contribute a
/// clean zero through an empty loop.
pub(crate) fn scan_check_contradictions(check: &Value) -> Result<(usize, Vec<String>), String> {
    if findings_rendering_truncated(check) {
        return Err(
            "findings array is a rendering prefix; trust cannot inspect omitted findings"
                .to_string(),
        );
    }
    let Some(findings) = check.get("findings").and_then(Value::as_array) else {
        return Err("check document is missing a findings array".to_string());
    };
    let mut count = 0;
    let mut examples = Vec::new();
    for finding in findings {
        let related = finding["related_tests_total"].as_u64().unwrap_or(0);
        let id = finding["id"].as_str().unwrap_or("?");
        let rule = if finding["classification"].as_str() == Some("no_static_path") && related > 0 {
            Some(format!(
                "R2 {id} no_static_path with {related} related tests"
            ))
        } else if related == 0
            && finding["evidence"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .any(|line| line.starts_with("Related tests were found"))
        {
            Some(format!("R3 {id} says related tests were found but lists 0"))
        } else {
            None
        };
        if let Some(rule) = rule {
            count += 1;
            if examples.len() < 3 {
                examples.push(rule);
            }
        }
    }
    Ok((count, examples))
}

pub(crate) fn findings_rendering_truncated(check: &Value) -> bool {
    check["run_limitations"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|limitation| {
            limitation["run_status"].as_str() == Some(FINDINGS_BOUND_STATUS)
                || limitation["category"].as_str() == Some(FINDINGS_BOUND_STATUS)
        })
}

fn finding_count(
    envelope: &serde_json::Map<String, Value>,
    outcome: &serde_json::Map<String, Value>,
) -> Option<u64> {
    outcome
        .get("counts")
        .and_then(Value::as_object)
        .and_then(|counts| counts.get("finding_count"))
        .and_then(Value::as_u64)
        .or_else(|| {
            envelope
                .get("summary")
                .and_then(Value::as_object)
                .and_then(|summary| summary.get("findings"))
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            envelope
                .get("findings")
                .and_then(Value::as_array)
                .map(|findings| findings.len() as u64)
        })
}

fn rejected(reason: &str) -> CheckAdmission {
    CheckAdmission::Rejected {
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn subject() -> CheckSubject {
        CheckSubject {
            root: "/tmp/corpus-a".to_string(),
            mode: "draft".to_string(),
            base: "HEAD~1".to_string(),
            head: Some("abc123".to_string()),
        }
    }

    fn complete_outcome(kind: &str, finding_count: u64) -> Value {
        json!({
            "schema_version": OUTCOME_SCHEMA_VERSION,
            "kind": kind,
            "identity": {
                "repository_identity": null,
                "root_identity": null,
                "config_identity": null,
                "base_revision": "HEAD~1",
                "input_identity": null,
                "snapshot_identity": null,
                "git_candidate_subject": null
            },
            "counts": {
                "changed_file_count": 1,
                "changed_line_count": 1,
                "candidate_line_count": if kind == "complete_no_findings" { 1 } else { 0 },
                "probe_count": if kind == "complete_no_findings" { 1 } else { 0 },
                "finding_count": finding_count
            },
            "limitations": [],
            "claim_boundary": OUTCOME_CLAIM_BOUNDARY
        })
    }

    fn complete_document(kind: &str, findings: Vec<Value>) -> Value {
        let finding_count = findings.len() as u64;
        json!({
            "schema_version": CHECK_SCHEMA_VERSION,
            "tool": PRODUCER_TOOL,
            "mode": "draft",
            "root": "/tmp/corpus-a",
            "base": "HEAD~1",
            "head": { "source": "commit", "commit": "abc123" },
            "summary": { "findings": finding_count },
            "findings": findings,
            "analysis_outcome": {
                "analysis_complete": true,
                "outcome": complete_outcome(kind, finding_count)
            }
        })
    }

    fn one_finding() -> Value {
        json!({
            "id": "p1",
            "classification": "weakly_exposed",
            "related_tests_total": 1,
            "evidence": []
        })
    }

    #[test]
    fn empty_and_null_json_are_not_complete_work() {
        let subject = subject();
        for stdout in ["{}", "null", "[]", "1", "\"ok\"", ""] {
            assert!(
                permissive_json_exit_zero(false, true, stdout) || stdout.is_empty(),
                "{stdout:?} should show the old predicate's hole when it parses"
            );
            assert!(
                !admit_check_sample(false, true, stdout, &subject).is_complete(),
                "{stdout:?} must not be a successful sample"
            );
        }
        assert!(permissive_json_exit_zero(false, true, "{}"));
        assert!(permissive_json_exit_zero(false, true, "null"));
        assert!(!permissive_json_exit_zero(false, true, ""));
    }

    #[test]
    fn missing_envelope_fields_are_rejected() {
        let subject = subject();
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc.as_object_mut().unwrap().remove("analysis_outcome");
        assert!(!admit_check_document(&doc, &subject).is_complete());
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["summary"] = json!(null);
        assert!(!admit_check_document(&doc, &subject).is_complete());
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc.as_object_mut().unwrap().remove("findings");
        assert!(!admit_check_document(&doc, &subject).is_complete());
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["tool"] = json!("other");
        assert!(!admit_check_document(&doc, &subject).is_complete());
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["schema_version"] = json!("9.9");
        assert!(!admit_check_document(&doc, &subject).is_complete());
    }

    #[test]
    fn incomplete_and_contradictory_outcomes_are_rejected() {
        let subject = subject();
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["analysis_outcome"]["analysis_complete"] = json!(false);
        doc["analysis_outcome"]["outcome"]["kind"] = json!("partial_with_limitations");
        assert!(!admit_check_document(&doc, &subject).is_complete());
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["analysis_outcome"]["analysis_complete"] = json!(true);
        doc["analysis_outcome"]["outcome"]["kind"] = json!("partial_with_limitations");
        match admit_check_document(&doc, &subject) {
            CheckAdmission::Rejected { reason } => {
                assert!(
                    reason.contains("does not match typed outcome kind"),
                    "{reason}"
                );
            }
            other => panic!("expected contradictory completeness reject, got {other:?}"),
        }
        for kind in ["unsupported_input", "analysis_failed"] {
            let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
            doc["analysis_outcome"]["analysis_complete"] = json!(false);
            doc["analysis_outcome"]["outcome"]["kind"] = json!(kind);
            assert!(
                !admit_check_document(&doc, &subject).is_complete(),
                "{kind} must not improve the baseline"
            );
        }
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["analysis_outcome"]["analysis_complete"] = json!("yes");
        assert!(!admit_check_document(&doc, &subject).is_complete());
    }

    #[test]
    fn wrong_subject_is_rejected() {
        let subject = subject();
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["root"] = json!("/tmp/corpus-b");
        assert!(!admit_check_document(&doc, &subject).is_complete());
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["mode"] = json!("ready");
        assert!(!admit_check_document(&doc, &subject).is_complete());
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["base"] = json!("main");
        assert!(!admit_check_document(&doc, &subject).is_complete());
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["head"]["commit"] = json!("fff999");
        assert!(!admit_check_document(&doc, &subject).is_complete());
    }

    #[test]
    fn unknown_head_is_preserved() {
        let mut subject = subject();
        subject.head = None;
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc.as_object_mut().unwrap().remove("head");
        assert!(admit_check_document(&doc, &subject).is_complete());
    }

    #[test]
    fn valid_complete_output_is_admitted() {
        let subject = subject();
        let with_findings = complete_document("complete_with_findings", vec![one_finding()]);
        match admit_check_document(&with_findings, &subject) {
            CheckAdmission::Complete {
                findings,
                rendering_truncated,
                kind,
            } => {
                assert_eq!(findings, 1);
                assert!(!rendering_truncated);
                assert_eq!(kind, "complete_with_findings");
            }
            other => panic!("expected complete admission, got {other:?}"),
        }
        let empty = complete_document("complete_no_findings", Vec::new());
        assert!(admit_check_document(&empty, &subject).is_complete());
        let no_behavior = complete_document("no_behavioral_candidates", Vec::new());
        assert!(admit_check_document(&no_behavior, &subject).is_complete());
    }

    #[test]
    fn rendering_truncation_is_not_incomplete_analysis() {
        let subject = subject();
        let mut doc = complete_document("complete_with_findings", vec![one_finding()]);
        doc["run_limitations"] = json!([{
            "category": FINDINGS_BOUND_STATUS,
            "run_status": FINDINGS_BOUND_STATUS,
            "downstream_consumable": false
        }]);
        match admit_check_document(&doc, &subject) {
            CheckAdmission::Complete {
                rendering_truncated,
                ..
            } => assert!(rendering_truncated),
            other => panic!("truncated complete analysis must stay complete: {other:?}"),
        }
        assert!(scan_check_contradictions(&doc).is_err());
    }

    #[test]
    fn missing_findings_cannot_yield_a_clean_zero() {
        assert!(scan_check_contradictions(&json!({})).is_err());
        assert!(scan_check_contradictions(&json!({"findings": null})).is_err());
        let ok = json!({"findings": [
            {"id": "p1", "classification": "no_static_path", "related_tests_total": 2, "evidence": []}
        ]});
        assert_eq!(scan_check_contradictions(&ok).unwrap().0, 1);
    }

    #[test]
    fn timeout_and_nonzero_exit_keep_rejection() {
        let subject = subject();
        let stdout = complete_document("complete_with_findings", vec![one_finding()]).to_string();
        assert!(!admit_check_sample(true, true, &stdout, &subject).is_complete());
        assert!(!admit_check_sample(false, false, &stdout, &subject).is_complete());
    }
}
