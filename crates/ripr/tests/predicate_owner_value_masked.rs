//! Public-API controls for a predicate assertion that calls the changed
//! function but cannot observe the call's value (#14206).
//!
//! The owner is `pub fn over(qty: u32) -> bool { qty >= 10 }`, changed from
//! `qty > 10`. Each case is the whole body of the one test in the crate and
//! calls `over(10)`, the input the two versions disagree on. A body whose
//! compared value does not carry the call's result passes for either
//! version, so the predicate finding must not read `exposed`; a body whose
//! compared value does carry it keeps `exposed`. Every case writes a small
//! crate and reads the predicate finding through `check_workspace`.
//!
//! Static evidence only: no case here claims a runtime mutation outcome.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-predicate-owner-value-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // The guard owns only a directory this call created, and owns it
        // before anything is written inside.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A small subject shared by the diff and repo-scope readings.
fn subject(body: &str) -> Result<Scratch, String> {
    let scratch = Scratch::create()?;
    let manifest =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
    let lib = format!(
        "pub fn over(qty: u32) -> bool {{\n    qty >= 10\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn first(a: bool, _b: bool) -> bool {{\n        a\n    }}\n\n    #[test]\n    fn t() {{\n        let _ = first(true, true);\n        {body}\n    }}\n}}\n"
    );

    std::fs::write(scratch.0.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;

    Ok(scratch)
}

fn predicate_finding(body: &str) -> Result<Value, String> {
    let scratch = subject(body)?;
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-    qty > 10\n+    qty >= 10\n";
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))
}

/// Bodies whose assertion passes for either version of `over`.
const MASKED: &[&str] = &[
    "assert_eq!(Some(over(10)).is_some(), true);",
    "assert_eq!(vec![over(10)].len(), 1);",
    "assert_eq!((over(10), 1).1, 1);",
    "assert_eq!({ over(10); true }, true);",
    "assert_eq!(first(true, over(10)), true);",
    "assert_eq!(Some(true).unwrap_or(over(10)), true);",
    "assert_eq!(Some(true).unwrap_or_else(|| over(10)), true);",
    "let r: Result<bool, ()> = Ok(true); assert_eq!(r.unwrap_or_else(|_| over(10)), true);",
    "assert_eq!(false.then(|| over(10)), None);",
    "let o: Option<u32> = None; assert_eq!(o.map(|_| over(10)), None);",
    "assert_eq!(None::<u32>.map_or(true, |_| over(10)), true);",
    "assert_eq!(if true { true } else { over(10) }, true);",
    "let v = match Some(1) { Some(_) => true, None => over(10) }; assert_eq!(v, true);",
    "assert_eq!(std::iter::repeat_with(|| over(10)).take(0).count(), 0);",
    "assert!(true || over(10));",
];

/// Bodies whose compared value carries the owner call's result.
const OBSERVING: &[&str] = &[
    "assert_eq!(over(10), true);",
    "assert!(over(10)); assert!(!over(9));",
    "assert_eq!(over(10) as u8, 1);",
    "let a = over(10); assert_eq!(a, true);",
    "assert_eq!((over(10), over(9)), (true, false));",
    "assert_eq!(Some(over(10)), Some(true));",
    "assert_eq!(!over(10), false);",
    "assert_eq!(over(10).then_some(1), Some(1));",
    "assert_eq!(if over(10) { 1 } else { 2 }, 1);",
    "assert_eq!(match over(10) { true => 'y', false => 'n' }, 'y');",
    "assert!(over(10) == true);",
    "assert_eq!(format!(\"a, b ({})\", over(10)), \"a, b (true)\");",
    "assert!(matches!(over(10), true));",
    "assert_eq!(u8::from(over(10)), 1);",
    "assert_eq!(Some(over(10)).eq(&Some(true)), true);",
    "assert_eq!(vec![over(10)].contains(&true), true);",
    "assert_eq!(Some(over(10)).unwrap_or(false), true);",
    "assert_eq!([over(10)].first(), Some(&true));",
    "for (q, e) in [(10, true), (9, false)] { assert_eq!(over(q), e); }",
];

#[test]
fn an_assertion_that_discards_or_masks_the_owner_call_is_not_exposed() -> Result<(), String> {
    for body in MASKED {
        let finding = predicate_finding(body)?;
        assert_eq!(
            finding["classification"], "weakly_exposed",
            "{body}: {finding}"
        );
        let summary = finding["ripr"]["discriminate"]["summary"]
            .as_str()
            .ok_or("missing discriminate summary")?;
        assert!(
            summary.contains("discards or masks the call's value")
                && summary.contains("(observation_unverified)"),
            "{body}: {summary}"
        );
        assert_eq!(finding["ripr"]["discriminate"]["state"], "weak", "{body}");
    }
    Ok(())
}

#[test]
fn an_assertion_whose_compared_value_carries_the_owner_call_stays_exposed() -> Result<(), String> {
    for body in OBSERVING {
        let finding = predicate_finding(body)?;
        assert_eq!(finding["classification"], "exposed", "{body}: {finding}");
    }
    Ok(())
}

#[test]
fn an_observing_assertion_beside_a_masked_one_keeps_the_credit() -> Result<(), String> {
    for body in [
        "assert_eq!(Some(over(10)).is_some(), true); assert_eq!(over(10), true);",
        "assert_eq!(over(10), true); assert_eq!(Some(over(10)).is_some(), true);",
    ] {
        let finding = predicate_finding(body)?;
        assert_eq!(finding["classification"], "exposed", "{body}: {finding}");
    }
    Ok(())
}

/// The real repo renderer, including per-test grades and canonical repair
/// evidence, must agree about both seams on the owner's boolean tail (#15071).
fn assert_repo_grips(body: &str, expected: &str) -> Result<(), String> {
    assert_repo_grips_with_summary(body, expected, None)
}

fn assert_repo_grips_with_summary(
    body: &str,
    expected: &str,
    summary_fragment: Option<&str>,
) -> Result<(), String> {
    let scratch = subject(body)?;
    let output = ripr::app::repo_seam_inventory_input(CheckInput {
        root: scratch.0.clone(),
        mode: Mode::Fast,
        ..CheckInput::default()
    });
    // Exercise the compact path cold before the full renderer can populate caches.
    let summary = render_check(&output, &OutputFormat::RepoExposureSummaryJson)?;
    let summary: Value = serde_json::from_str(&summary).map_err(|error| error.to_string())?;
    assert_eq!(summary["metrics"]["raw_seams"], 2, "{body}: {summary}");
    assert_eq!(
        summary["metrics"]["grip_class"][expected], 2,
        "{body}: {summary}"
    );
    let rendered = render_check(&output, &OutputFormat::RepoExposureJson)?;
    let report: Value = serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
    assert_eq!(report["run_status"], "complete");
    let seams: Vec<_> = report["seams"]
        .as_array()
        .ok_or("missing repo seams")?
        .iter()
        .filter(|seam| seam["owner"] == "src/lib.rs::over")
        .collect();
    assert_eq!(
        seams.len(),
        2,
        "must inspect both actual owner seams: {report}"
    );
    for kind in ["predicate_boundary", "return_value"] {
        let seam = seams
            .iter()
            .find(|seam| seam["kind"] == kind)
            .ok_or_else(|| format!("missing {kind}: {report}"))?;
        assert_eq!(seam["grip_class"], expected, "{body}: {seam}");
        let strength = if expected == "strongly_gripped" {
            "strong"
        } else {
            "weak"
        };
        let related = seam["related_tests"]
            .as_array()
            .ok_or("missing related tests")?;
        assert_eq!(related.len(), 1, "{body}: {seam}");
        assert_eq!(related[0]["oracle_strength"], strength, "{body}: {seam}");
        let discriminate = &seam["evidence_record"]["evidence_path"]["discriminate"];
        if let Some(fragment) = summary_fragment {
            assert!(
                discriminate["summary"]
                    .as_str()
                    .is_some_and(|summary| summary.contains(fragment)),
                "{body}: {seam}"
            );
        }
        assert_eq!(
            discriminate["state"],
            if strength == "strong" { "yes" } else { "weak" },
            "{body}: {seam}"
        );
    }
    Ok(())
}

#[test]
fn repo_masked_owner_values_cannot_strongly_grip_either_tail_seam() -> Result<(), String> {
    for body in MASKED {
        assert_repo_grips(body, "weakly_gripped")?;
    }
    assert_repo_grips(
        "let got = Some(over(10)); assert_eq!(got.is_some(), true);",
        "weakly_gripped",
    )?;
    Ok(())
}

#[test]
fn repo_forcing_boolean_literals_mask_both_tail_seams() -> Result<(), String> {
    for body in [
        "assert_eq!(over(10) && false, false);",
        "assert_eq!(false && over(10), false);",
        "assert_eq!(over(10) || true, true);",
        "assert_eq!(true || over(10), true);",
    ] {
        assert_repo_grips_with_summary(body, "weakly_gripped", Some("masks the owner"))?;
    }
    Ok(())
}

#[test]
fn repo_or_identity_preserves_existing_relational_credit() -> Result<(), String> {
    for body in [
        "assert_eq!(over(10) || false, true);",
        "assert_eq!(false || over(10), true);",
    ] {
        assert_repo_grips_with_summary(body, "weakly_gripped", Some("kind-match true"))?;
    }
    Ok(())
}

#[test]
fn repo_observed_owner_values_keep_both_tail_seams_strong() -> Result<(), String> {
    for body in [
        "assert_eq!(over(10), true);",
        "assert_eq!(over(10) && true, true);",
        "assert_eq!(true && over(10), true);",
        "assert_eq!(Some(over(10)), Some(true));",
        "let got = Some(over(10)); assert_eq!(got, Some(true));",
        "assert_eq!((over(10), 1).0, true);",
    ] {
        assert_repo_grips(body, "strongly_gripped")?;
    }
    Ok(())
}

#[test]
fn repo_mem_replace_observes_only_the_old_temporary_on_both_seams() -> Result<(), String> {
    assert_repo_grips(
        "assert_eq!(std::mem::replace(&mut over(10), false), true);",
        "strongly_gripped",
    )?;
    for body in [
        "assert_eq!(std::mem::replace(&mut true, over(10)), true);",
        "let replace = |_: &mut bool, _: bool| true; assert_eq!(replace(&mut over(10), false), true);",
    ] {
        assert_repo_grips_with_summary(body, "weakly_gripped", Some("masks the owner"))?;
    }
    Ok(())
}

#[test]
fn repo_observing_assertion_keeps_credit_beside_a_masked_assertion() -> Result<(), String> {
    for body in [
        "assert_eq!(Some(over(10)).is_some(), true); assert_eq!(over(10), true);",
        "assert_eq!(over(10), true); assert_eq!(Some(over(10)).is_some(), true);",
    ] {
        assert_repo_grips(body, "strongly_gripped")?;
    }
    Ok(())
}
