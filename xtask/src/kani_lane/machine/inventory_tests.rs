//! Synthetic observation controls; no verifier execution is claimed.

use super::super::{HarnessStatus, lane_verdict, parse_args, results_from_output};
use super::{CaptureOutcome, Harness, InvocationContext, NEXT_RUN, Report, SourceObservation, digest};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let temporary = std::env::temp_dir();
        fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let root = temporary.join(format!(
            "kani-source-inventory-{}-{}",
            std::process::id(),
            NEXT_RUN.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }

    fn source(&self, filename: &str) -> Result<(String, String), String> {
        let path = self.0.join(filename);
        let text = "#[kani::proof]\nfn first() {}\n#[kani::proof]\nfn second() {}\n";
        fs::write(&path, text).map_err(|error| error.to_string())?;
        Ok((path.to_string_lossy().into_owned(), text.to_string()))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn harness(source: &(String, String), name: &str) -> Harness {
    Harness {
        file: source.0.clone(),
        name: name.to_string(),
    }
}

fn observation(source: &(String, String)) -> SourceObservation {
    SourceObservation {
        file: source.0.clone(),
        bytes: source.1.len(),
        sha256: digest(source.1.as_bytes()),
    }
}

fn report_for(
    harnesses: &[Harness],
    sources: &[(String, String)],
    allow_empty: bool,
) -> Result<Report, String> {
    let mut report = Report::new("synthetic-source-inventory".to_string());
    let arguments = if allow_empty {
        vec!["--allow-empty".to_string()]
    } else {
        Vec::new()
    };
    report.request(&parse_args(&arguments)?);
    report.select("ripr", sources, harnesses)?;
    report.availability(true);
    let output: String = harnesses
        .iter()
        .map(|harness| {
            format!(
                "Checking harness m::{}...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n",
                harness.name
            )
        })
        .collect();
    let rows = results_from_output(harnesses, &output);
    assert_eq!(lane_verdict(&rows, true, allow_empty), Ok(()));
    for package in ["@preflight", "ripr"] {
        if package == "ripr" && harnesses.is_empty() {
            continue;
        }
        report.capture(
            InvocationContext {
                package,
                argv: &[],
                budget: Some(Duration::from_secs(1)),
                elapsed: Duration::ZERO,
                harnesses,
            },
            CaptureOutcome {
                exit_code: Some(0),
                exit_success: Some(true),
                timed_out: false,
                stdout: Some(&output),
                stderr: Some(""),
                error: None,
            },
        );
    }
    report.rows("ripr", &rows);
    // Exercise real file reads; do not assign a successful recheck by hand.
    report.recheck_sources();
    Ok(report)
}

fn assert_inventory_refusal(report: &mut Report, reason: &str) {
    // These setup assertions isolate inventory admission from source-read,
    // native row and subprocess failures that could otherwise mask the hole.
    assert_eq!(report.selected_files_unchanged_after_run, Some(true));
    assert!(report.source_recheck_errors.is_empty());
    assert!(report.invocations.iter().all(|invocation| {
        invocation.exit_success == Some(true)
            && !invocation.timed_out
            && invocation.capture_error.is_none()
            && invocation.unmatched_native_blocks.is_empty()
    }));
    assert!(report.results.iter().all(|row| {
        matches!(row.observation.status, HarnessStatus::Verified)
            && row.claim_id.is_none()
            && row.claim_generation.is_none()
    }));
    assert_eq!(
        report.finish(Ok(())),
        Err(format!("NOT-ESTABLISHED: {reason}"))
    );
    assert!(!report.lane_success);
    assert_eq!(report.phase, "completed");
    assert_eq!(report.registered_claims, 0);
    assert_eq!(report.claim_assurance, "not_established");
    assert!(report.markdown().contains(reason));
}

#[test]
fn selected_file_without_source_observation_is_refused() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let first = fixture.source("first.rs")?;
    let second = fixture.source("second.rs")?;
    let harnesses = [harness(&first, "first"), harness(&second, "second")];
    for allow_empty in [false, true] {
        let mut report = report_for(&harnesses, std::slice::from_ref(&first), allow_empty)?;
        assert_eq!(report.selected.len(), 2);
        assert_eq!(report.results.len(), 2);
        assert_eq!(report.selected_source_files.len(), 1);
        assert_inventory_refusal(
            &mut report,
            "selected harness files and observed source files do not agree",
        );
    }
    Ok(())
}

#[test]
fn unselected_source_observation_is_refused() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let selected = fixture.source("selected.rs")?;
    let extra = fixture.source("extra.rs")?;
    for allow_empty in [false, true] {
        let mut report = report_for(
            &[harness(&selected, "first")],
            std::slice::from_ref(&selected),
            allow_empty,
        )?;
        report.selected_source_files.push(observation(&extra));
        report.recheck_sources();
        assert_inventory_refusal(
            &mut report,
            "selected harness files and observed source files do not agree",
        );
    }
    Ok(())
}

#[test]
fn duplicate_source_observations_are_refused() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let source = fixture.source("proof.rs")?;
    let mut report = report_for(
        &[harness(&source, "first")],
        std::slice::from_ref(&source),
        false,
    )?;
    report.selected_source_files.push(observation(&source));
    report.recheck_sources();
    assert_inventory_refusal(
        &mut report,
        "duplicate source-file observations are not admitted",
    );
    Ok(())
}

#[test]
fn same_cardinality_source_substitution_is_refused() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let first = fixture.source("first.rs")?;
    let second = fixture.source("second.rs")?;
    let replacement = fixture.source("replacement.rs")?;
    let mut report = report_for(
        &[harness(&first, "first"), harness(&second, "second")],
        &[first, second.clone()],
        false,
    )?;
    // Equal record counts do not establish file-set equality. Keep both
    // observed files readable so the recheck alone cannot catch substitution.
    let slot = report
        .selected_source_files
        .iter_mut()
        .find(|source| source.file == second.0)
        .ok_or("missing second source observation in positive setup")?;
    *slot = observation(&replacement);
    report.recheck_sources();
    assert_eq!(report.selected_source_files.len(), report.selected.len());
    assert_inventory_refusal(
        &mut report,
        "selected harness files and observed source files do not agree",
    );
    Ok(())
}

#[test]
fn multiple_harnesses_share_one_observed_source_file() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let source = fixture.source("shared.rs")?;
    let harnesses = [harness(&source, "first"), harness(&source, "second")];
    for allow_empty in [false, true] {
        let mut report = report_for(&harnesses, std::slice::from_ref(&source), allow_empty)?;
        assert_eq!(report.selected_source_files.len(), 1);
        assert_eq!(report.results.len(), 2);
        assert_eq!(report.selected_files_unchanged_after_run, Some(true));
        assert_eq!(report.finish(Ok(())), Ok(()));
        assert!(report.lane_success);
        assert_eq!(report.registered_claims, 0);
    }
    Ok(())
}

#[test]
fn complete_multi_file_inventory_is_order_independent() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let first = fixture.source("first.rs")?;
    let second = fixture.source("second.rs")?;
    let harnesses = [harness(&first, "first"), harness(&second, "second")];
    let mut forward = report_for(&harnesses, &[first.clone(), second.clone()], false)?;
    let mut reverse = report_for(
        &[harnesses[1].clone(), harnesses[0].clone()],
        &[second, first],
        false,
    )?;
    assert_eq!(forward.selected_source_files, reverse.selected_source_files);
    assert_eq!(
        forward.source_observation_digest,
        reverse.source_observation_digest
    );
    assert_eq!(forward.selected, reverse.selected);
    assert_eq!(forward.finish(Ok(())), Ok(()));
    assert_eq!(reverse.finish(Ok(())), Ok(()));
    Ok(())
}

#[test]
fn empty_opt_out_does_not_admit_an_orphan_source() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let extra = fixture.source("orphan.rs")?;
    let mut report = report_for(&[], &[], true)?;
    assert_eq!(report.finish(Ok(())), Ok(()));
    report.selected_source_files.push(observation(&extra));
    report.recheck_sources();
    assert_inventory_refusal(
        &mut report,
        "selected harness files and observed source files do not agree",
    );
    Ok(())
}

#[test]
fn complete_inventory_does_not_bypass_changed_source_refusal() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let first = fixture.source("first.rs")?;
    let second = fixture.source("second.rs")?;
    let harnesses = [harness(&first, "first"), harness(&second, "second")];
    let mut report = report_for(&harnesses, &[first, second.clone()], false)?;
    fs::write(&second.0, "// changed after selection\n").map_err(|error| error.to_string())?;
    report.recheck_sources();
    assert_eq!(report.selected_files_unchanged_after_run, Some(false));
    assert_eq!(
        report.finish(Ok(())),
        Err("NOT-ESTABLISHED: discovered proof source changed or could not be rechecked".to_string())
    );
    assert!(!report.lane_success);
    Ok(())
}
