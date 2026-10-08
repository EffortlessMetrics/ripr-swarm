//! End-to-end delegation pins for Python family-relevant assertion selection
//! (RIPR-SPEC-0224, #5572): each case runs the production `check` path on an
//! on-disk workspace and feeds the JSON check output to the gap ledger, so the
//! real agent-packet decision is exercised.

use crate::output::gap_decision_ledger::{
    GapDecisionLedgerInput, GapDecisionLedgerSourceKind, build_gap_decision_ledger_report,
    render_gap_decision_ledger_json,
};
use serde_json::Value;
use std::fs;

const PYTHON_CONFIG: &str = "[languages]\nenabled = [\"rust\", \"python\"]\n";

struct PythonCheck {
    findings: Vec<Value>,
    ledger: Value,
}

impl PythonCheck {
    fn only_finding(&self) -> Result<&Value, String> {
        match self.findings.as_slice() {
            [finding] => Ok(finding),
            other => Err(format!("expected one finding, got {}", other.len())),
        }
    }

    fn agent_packet_eligible(&self, finding: &Value) -> Result<bool, String> {
        let gap_id = finding
            .get("canonical_gap_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("finding has no canonical gap id: {finding}"))?;
        let record = self
            .ledger
            .get("records")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|record| record.get("canonical_gap_id").and_then(Value::as_str) == Some(gap_id))
            .ok_or_else(|| format!("no ledger record for {gap_id}: {}", self.ledger))?;
        record
            .get("projection_eligibility")
            .and_then(|projection| projection.get("agent_packet"))
            .and_then(|packet| packet.get("eligible"))
            .and_then(Value::as_bool)
            .ok_or_else(|| format!("ledger record has no agent_packet eligibility: {record}"))
    }
}

fn str_field<'a>(finding: &'a Value, key: &str) -> Option<&'a str> {
    finding.get(key).and_then(Value::as_str)
}

fn check_python(label: &str, files: &[(&str, &str)], diff: &str) -> Result<PythonCheck, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-family-{label}-{}-{stamp}",
        std::process::id()
    ));
    let output = (|| -> Result<String, String> {
        for (path, contents) in files {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::write(&path, contents)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
        fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff fixture: {error}"))?;
        let config = crate::config::tests_only_parse(PYTHON_CONFIG)?;
        let output = crate::app::check_workspace_with_config(
            crate::CheckInput {
                root: root.clone(),
                base: None,
                diff_file: Some(root.join("diff.patch")),
                mode: crate::Mode::Draft,
                format: crate::OutputFormat::Json,
                include_unchanged_tests: true,
                perl_facts_path: None,
                suppression_policy: None,
                git_timeout: None,
                git_candidate: None,
            },
            &config,
        )?;
        crate::render_check(&output, &crate::OutputFormat::Json)
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove fixture {}: {error}", root.display()));
    let check_output = output?;
    cleanup?;
    let value: Value = serde_json::from_str(&check_output)
        .map_err(|error| format!("parse check output: {error}"))?;
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let report = build_gap_decision_ledger_report(GapDecisionLedgerInput {
        root: format!("source-fixture/{label}"),
        generated_at: "test".to_string(),
        source_kind: GapDecisionLedgerSourceKind::CheckOutput,
        records_path: format!("{label}.json"),
        records_json: Ok(check_output),
    });
    let ledger = render_gap_decision_ledger_json(&report)?;
    let ledger: Value =
        serde_json::from_str(&ledger).map_err(|error| format!("parse ledger: {error}"))?;
    Ok(PythonCheck { findings, ledger })
}

/// Assert a weakly exposed card whose rows lost their strength-only oracle
/// is advisory only.
fn assert_not_delegated(check: &PythonCheck, reason: &str) -> Result<(), String> {
    let finding = check.only_finding()?;
    if str_field(finding, "classification") != Some("weakly_exposed")
        || str_field(finding, "alignment_reason") != Some(reason)
        || finding.get("python_repair_card").is_none()
    {
        return Err(format!(
            "expected a weakly exposed `{reason}` repair card: {finding}"
        ));
    }
    if check.agent_packet_eligible(finding)? {
        return Err(format!("`{reason}` card was delegated: {finding}"));
    }
    Ok(())
}

const PARSE_RAISE_DIFF: &str = "diff --git a/src/app.py b/src/app.py\nindex 1111111..2222222 100644\n--- a/src/app.py\n+++ b/src/app.py\n@@ -1,4 +1,4 @@\n def parse(text):\n     if not text:\n-        raise ValueError(\"empty\")\n+        raise KeyError(\"empty\")\n     return int(text)\n";

/// A same-named method on another class, observed only by an exception
/// assertion, must not hand an agent a repair aimed at that other owner's
/// test (review probe W).
#[test]
fn no_family_relevant_row_for_same_method_on_other_class_stays_advisory() -> Result<(), String> {
    let check = check_python(
        "same-method-other-class",
        &[
            (
                "src/auth.py",
                "class TokenValidator:\n    def __init__(self, valid):\n        self._valid = valid\n\n    def validate(self, token):\n        return token.strip() in self._valid\n",
            ),
            (
                "src/billing.py",
                "class PaymentProcessor:\n    def validate(self, card):\n        return len(card.strip()) == 9\n",
            ),
            (
                "tests/test_billing.py",
                "import pytest\nfrom src.billing import PaymentProcessor\n\n\ndef test_billing_validate():\n    proc = PaymentProcessor()\n    with pytest.raises(AttributeError, match=\"strip\"):\n        proc.validate(None)\n",
            ),
        ],
        "diff --git a/src/auth.py b/src/auth.py\nindex 0000000..1111111 100644\n--- a/src/auth.py\n+++ b/src/auth.py\n@@ -3,4 +3,4 @@ class TokenValidator:\n         self._valid = valid\n\n     def validate(self, token):\n-        return token in self._valid\n+        return token.strip() in self._valid\n",
    )?;
    assert_not_delegated(&check, "no_family_relevant_assertion")
}

/// A changed raise whose only related assertion observes a wrapper's normal
/// value (review probe B).
#[test]
fn no_family_relevant_row_for_wrapper_value_on_changed_raise_stays_advisory() -> Result<(), String>
{
    let check = check_python(
        "wrapper-value-changed-raise",
        &[
            ("src/__init__.py", ""),
            (
                "src/app.py",
                "def parse(text):\n    if not text:\n        raise KeyError(\"empty\")\n    return int(text)\n\n\ndef total(a):\n    return parse(a) + 1\n",
            ),
            ("tests/__init__.py", ""),
            (
                "tests/test_app.py",
                "from src.app import parse, total\n\ndef test_total():\n    parse(\"1\")\n    assert total(\"2\") == 3\n",
            ),
        ],
        PARSE_RAISE_DIFF,
    )?;
    assert_not_delegated(&check, "no_family_relevant_assertion")?;
    let finding = check.only_finding()?;
    let missing = finding
        .get("missing")
        .and_then(Value::as_array)
        .and_then(|missing| missing.first())
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !missing.contains("observe a different behavior (strongest: `exact_value`)")
        || !missing.contains("the exact exception type/message")
    {
        return Err(format!(
            "missing prose does not name what happened: {missing}"
        ));
    }
    Ok(())
}

/// A changed return whose only related assertion is an exception assertion
/// (review probe C).
#[test]
fn no_family_relevant_row_for_exception_on_changed_return_stays_advisory() -> Result<(), String> {
    let check = check_python(
        "exception-changed-return",
        &[
            ("src/__init__.py", ""),
            (
                "src/app.py",
                "def parse(text):\n    if not text:\n        raise ValueError(\"empty\")\n    return int(text) * 2\n",
            ),
            ("tests/__init__.py", ""),
            (
                "tests/test_app.py",
                "import pytest\nfrom src.app import parse\n\ndef test_parse_empty():\n    with pytest.raises(ValueError, match=\"empty\"):\n        parse(\"\")\n",
            ),
        ],
        "diff --git a/src/app.py b/src/app.py\nindex 1111111..2222222 100644\n--- a/src/app.py\n+++ b/src/app.py\n@@ -1,4 +1,4 @@\n def parse(text):\n     if not text:\n         raise ValueError(\"empty\")\n-    return int(text)\n+    return int(text) * 2\n",
    )?;
    assert_not_delegated(&check, "no_family_relevant_assertion")
}

/// A free-function return change whose test holds a strong exception
/// assertion and a weak value assertion: the row now shows the value
/// assertion, so the card is weakly exposed but not newly delegated.
#[test]
fn passed_over_stronger_assertion_card_is_not_newly_delegated() -> Result<(), String> {
    let check = check_python(
        "passed-over-stronger",
        &[
            ("src/__init__.py", ""),
            (
                "src/app.py",
                "def scale(x):\n    if x < 0:\n        raise ValueError(\"neg\")\n    return x * 3\n",
            ),
            ("tests/__init__.py", ""),
            (
                "tests/test_app.py",
                "import pytest\nfrom src.app import scale\n\ndef test_scale():\n    with pytest.raises(ValueError, match=\"neg\"):\n        scale(-1)\n    assert scale(2) > 0\n",
            ),
        ],
        "diff --git a/src/app.py b/src/app.py\nindex 1111111..2222222 100644\n--- a/src/app.py\n+++ b/src/app.py\n@@ -1,4 +1,4 @@\n def scale(x):\n     if x < 0:\n         raise ValueError(\"neg\")\n-    return x * 2\n+    return x * 3\n",
    )?;
    assert_not_delegated(&check, "other_behavior_assertion_passed_over")?;
    let finding = check.only_finding()?;
    let oracle = finding
        .get("related_tests")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("oracle"))
        .and_then(Value::as_str);
    if oracle != Some("assert scale(2) > 0") {
        return Err(format!("row must show the value assertion: {finding}"));
    }
    Ok(())
}

/// The class of a changed raise must not depend on whether the exception
/// assertion or the normal-value assertion comes last in the test.
#[test]
fn error_path_class_is_independent_of_assertion_source_order() -> Result<(), String> {
    let source = "def parse(text):\n    if not text:\n        raise KeyError(\"empty\")\n    return int(text)\n";
    let raises = "    with pytest.raises(KeyError, match=\"empty\"):\n        parse(\"\")\n";
    let value = "    assert parse(\"42\") == 42\n";
    let mut classes = Vec::new();
    for (label, body) in [
        ("raises-first", format!("{raises}{value}")),
        ("value-first", format!("{value}{raises}")),
    ] {
        let test = format!("import pytest\nfrom src.app import parse\n\ndef test_parse():\n{body}");
        let check = check_python(
            label,
            &[
                ("src/__init__.py", ""),
                ("src/app.py", source),
                ("tests/__init__.py", ""),
                ("tests/test_app.py", &test),
            ],
            PARSE_RAISE_DIFF,
        )?;
        let finding = check.only_finding()?;
        classes.push(str_field(finding, "classification").map(str::to_string));
    }
    let exposed = Some("exposed".to_string());
    if classes != vec![exposed.clone(), exposed] {
        return Err(format!(
            "a matching exception assertion must expose the raise in both orders: {classes:?}"
        ));
    }
    Ok(())
}
