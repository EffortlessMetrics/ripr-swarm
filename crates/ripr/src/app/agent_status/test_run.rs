//! The test-run axis of an issued repair receipt (#14079): what status says
//! about a focused run. ripr runs no test, so the only run it can report is
//! the one the caller reported and the finished attempt retained.

use super::{test_not_run_json, test_not_run_next_step};
use crate::app::repair_attempt::RepairAttemptManifest;
use crate::app::repair_attempt::retained_run::retained_focused_run;
use crate::output::agent_receipt::{AgentReceiptReading, ReportedTestRun};
use serde_json::Value;

/// The reading of the receipt issued for `manifest`'s attempt. The retained
/// caller-reported run rides the reading of the receipt issued for this same
/// attempt, never another's.
pub(super) fn issued_reading(
    manifest: &RepairAttemptManifest,
    value: &Value,
) -> AgentReceiptReading {
    let mut reading = AgentReceiptReading::from_value(value);
    reading.reported_test_run = retained_focused_run(manifest);
    reading
}

/// The `test_run` object for the receipt issued to `manifest`'s attempt, for
/// a transport that projects one attempt (#16251). It is the object `ripr agent
/// status --attempt` emits, built from the same retained record, and is `None`
/// when the receipt does not record that ripr ran no test.
pub(crate) fn issued_test_run_json(
    manifest: &RepairAttemptManifest,
    value: &Value,
) -> Option<Value> {
    let reading = issued_reading(manifest, value);
    reading.test_not_run().then(|| test_run_json(&reading))
}

/// The counts of a caller-reported run, in the words the finish printed.
pub(crate) fn reported_test_run_counts(run: &ReportedTestRun) -> String {
    format!(
        "{} test(s) ran in `{}` ({} passed, {} failed)",
        run.ran, run.file, run.passed, run.failed
    )
}

/// What a caller-reported run leaves to do. A reported failure is named:
/// static movement `improved` never turns a failing test into a pass.
pub(crate) fn reported_test_run_next_step(run: &ReportedTestRun) -> String {
    if run.failed > 0 {
        "The reported run has a failing test: make the focused test pass before keeping it. Static movement `improved` does not establish a passing test.".to_string()
    } else {
        "ripr checked that the result is bound to this attempt, its HEAD and the test file; it did not run or observe the tests, and the receipt's static movement is separate evidence.".to_string()
    }
}

/// The finish's next step once the caller reported a focused run (#16263).
///
/// An `advisory` receipt whose static movement is `improved` asks the caller
/// to run the focused test; that guidance is the receipt's static-only
/// default and stays in its digest-bound bytes. When the finish recorded a
/// run, the step is already done (a pass) or already failed (a failure), so
/// the summary names what is left instead. Any other receipt keeps its own
/// step: only it decides whether it is review evidence.
pub(crate) fn next_step_after_reported_run(
    run: Option<&ReportedTestRun>,
    receipt_status: Option<&str>,
    movement: Option<&str>,
    receipt_step: &str,
) -> String {
    match run {
        Some(run) if receipt_status == Some("advisory") && movement == Some("improved") => {
            if run.failed > 0 {
                "Make the focused test pass before keeping it; ripr did not run it. Then include this receipt in review.".to_string()
            } else {
                "Keep the focused test (the caller reported it passing; ripr did not run it) and include this receipt in review.".to_string()
            }
        }
        _ => receipt_step.to_string(),
    }
}

/// The status line for the test-run axis of an issued receipt: the caller's
/// reported run when the attempt retained one, otherwise that none is
/// recorded. Both say ripr ran nothing.
pub(super) fn test_run_line(reading: &AgentReceiptReading) -> String {
    match &reading.reported_test_run {
        Some(run) => format!(
            "Test run: reported by the caller, not run by ripr: {}. {}",
            reported_test_run_counts(run),
            reported_test_run_next_step(run)
        ),
        None => format!(
            "Test run: none recorded. {}",
            test_not_run_next_step(reading)
        ),
    }
}

/// The same line for one of several finished attempts in the aggregate
/// status (#14146): it names the attempt it speaks for.
pub(super) fn attempt_test_run_line(attempt_id: &str, reading: &AgentReceiptReading) -> String {
    let line = test_run_line(reading);
    match line.strip_prefix("Test run: ") {
        Some(rest) => format!("Test run (attempt `{attempt_id}`): {rest}"),
        None => line,
    }
}

/// How telling a reading is for the aggregate status (#14146), lowest first:
/// a caller-reported failure, then no recorded run, then a caller-reported
/// pass. A failure must never sit behind another attempt's reading.
pub(super) fn aggregate_rank(reading: &AgentReceiptReading) -> u8 {
    match &reading.reported_test_run {
        Some(run) if run.failed > 0 => 0,
        None => 1,
        Some(_) => 2,
    }
}

/// The aggregate JSON `test_run` object: the per-attempt object plus the
/// `attempt_id` it speaks for (#14146).
pub(super) fn aggregate_test_run_json(attempt_id: &str, reading: &AgentReceiptReading) -> Value {
    let mut value = test_run_json(reading);
    if let Some(object) = value.as_object_mut() {
        object.insert("attempt_id".to_string(), Value::from(attempt_id));
    }
    value
}

/// The JSON `test_run` object for the same axis. `not_recorded` keeps its
/// prior shape; `caller_reported` adds the reported counts.
pub(super) fn test_run_json(reading: &AgentReceiptReading) -> Value {
    match &reading.reported_test_run {
        Some(run) => serde_json::json!({
            "status": "caller_reported",
            "test_changed": reading.test_changed,
            "file": run.file,
            "ran": run.ran,
            "passed": run.passed,
            "failed": run.failed,
            "next_step": reported_test_run_next_step(run)
        }),
        None => test_not_run_json(reading),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(run: Option<ReportedTestRun>) -> AgentReceiptReading {
        let mut reading = AgentReceiptReading::from_value(&serde_json::json!({}));
        reading.reported_test_run = run;
        reading
    }

    fn run(passed: usize, failed: usize) -> ReportedTestRun {
        ReportedTestRun {
            file: "tests/pricing.rs".to_string(),
            ran: passed + failed,
            passed,
            failed,
        }
    }

    #[test]
    fn a_reported_run_replaces_only_the_improved_advisory_run_the_test_step() {
        let step = "Run the focused test and keep it only if it passes.";
        let advisory = Some("advisory");
        let improved = Some("improved");
        let passed = run(1, 0);
        let failed = run(0, 1);
        assert_eq!(
            next_step_after_reported_run(None, advisory, improved, step),
            step
        );
        let kept = next_step_after_reported_run(Some(&passed), advisory, improved, step);
        assert!(kept.starts_with("Keep the focused test"), "{kept}");
        assert!(!kept.contains("Run the focused test"), "{kept}");
        let fix = next_step_after_reported_run(Some(&failed), advisory, improved, step);
        assert!(fix.starts_with("Make the focused test pass"), "{fix}");
        for (status, movement) in [
            (Some("limited"), improved),
            (advisory, Some("unchanged")),
            (None, improved),
        ] {
            assert_eq!(
                next_step_after_reported_run(Some(&passed), status, movement, step),
                step
            );
        }
    }

    #[test]
    fn a_reported_run_is_read_back_with_its_counts_and_a_failure_is_named() {
        let none = reading(None);
        assert!(test_run_line(&none).starts_with("Test run: none recorded. "));
        assert_eq!(test_run_json(&none)["status"], "not_recorded");
        assert_eq!(test_run_json(&none).get("ran"), None);

        let passed = reading(Some(run(2, 0)));
        let line = test_run_line(&passed);
        assert!(
            line.starts_with(
                "Test run: reported by the caller, not run by ripr: 2 test(s) ran in `tests/pricing.rs` (2 passed, 0 failed). "
            ),
            "{line}"
        );
        assert!(line.contains("did not run or observe the tests"), "{line}");
        assert!(!line.contains("failing test"), "{line}");
        let json = test_run_json(&passed);
        assert_eq!(json["status"], "caller_reported");
        assert_eq!(json["file"], "tests/pricing.rs");
        assert_eq!(json["ran"], 2);
        assert_eq!(json["passed"], 2);
        assert_eq!(json["failed"], 0);

        let failed = reading(Some(run(1, 1)));
        let line = test_run_line(&failed);
        assert!(line.contains("(1 passed, 1 failed)"), "{line}");
        assert!(
            line.contains("The reported run has a failing test"),
            "{line}"
        );
        let json = test_run_json(&failed);
        assert_eq!(json["failed"], 1);
        assert_eq!(
            json["next_step"],
            reported_test_run_next_step(&run(1, 1)).as_str()
        );
    }
}
