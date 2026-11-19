//! Per-change run-time guard on classifier work (#15049).
//!
//! Run time is otherwise judged only by nightly lanes, because wall time
//! cannot be judged per change without flaking. This counts work instead: the
//! five-gap diff that `agent_output_budget` measures is checked once with
//! `RIPR_PROBE_READ_TRACE` set, and the indexed reads the classifier reports
//! for each probe are summed. A change that re-reads the index per probe, per
//! test or per oracle multiplies that sum and fails here in the same change.
//!
//! The check runs on one worker. Workers race to fill the shared run memo, so
//! with several of them the sum moves by about 1.5% from run to run; with one
//! it is the same number every run, with a cold or a warm cache.
//!
//! Measured 2026-11-18 on Linux x86_64: 8 rows and 996 reads in 44 of 44
//! one-worker runs; 1002 to 1011 reads in 50 four-worker runs.
//!
//! The ceiling has about 15% headroom on purpose. Analyzer changes add a few
//! reads for good reasons, and a pin that every such change must bump stops
//! meaning anything. A deliberate cost raises the ceiling in the same change
//! and says why; a deliberate saving lowers it. Run this module with
//! `--nocapture` to print the `check-work-budget` line.
//!
//! Not covered: time spent inside one read, index construction, parsing, and
//! every language other than Rust.
use super::agent_output_budget::workspace;
use super::run_command_with_env;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const TRACE_ENV: &str = "RIPR_PROBE_READ_TRACE";
const ROW_PREFIX: &str = "ripr_probe_read_trace ";

/// Classifier calls the five-gap diff produces: one row each.
const PROBES: usize = 8;

/// Total indexed reads across all rows. Measured: 996.
const READS_CEILING: u64 = 1_150;

/// Below this the trace has stopped counting or the subject has shrunk. A
/// real saving this large lowers both bounds in the same change.
const READS_FLOOR: u64 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Row {
    reads: u64,
    complete: bool,
}

/// Every probe-read row on `stderr`. A row that does not parse or lacks a
/// field is an error: a count that was not reported is never read as zero.
///
/// `observed_read_events` counts every read. The row's `dropped_read_events`
/// only says how many of them are missing from its per-site detail (the row
/// keeps 64 distinct reads), so it is not a reason to distrust the total and
/// is not read here.
fn rows_from_stderr(stderr: &str) -> Result<Vec<Row>, String> {
    stderr
        .lines()
        .filter_map(|line| line.strip_prefix(ROW_PREFIX))
        .map(|body| {
            let row: serde_json::Value = serde_json::from_str(body)
                .map_err(|error| format!("probe-read row is not JSON: {error}: {body}"))?;
            let count = |key: &str| {
                row[key]
                    .as_u64()
                    .ok_or_else(|| format!("probe-read row has no unsigned `{key}`: {body}"))
            };
            let flag = |key: &str| {
                row[key]
                    .as_bool()
                    .ok_or_else(|| format!("probe-read row has no boolean `{key}`: {body}"))
            };
            Ok(Row {
                reads: count("observed_read_events")?,
                complete: flag("classification_returned")? && !flag("interrupted")?,
            })
        })
        .collect()
}

/// The verdict for one run's rows. The only place a bound is applied; returns
/// the total it judged.
fn judge(rows: &[Row]) -> Result<u64, String> {
    if rows.len() != PROBES {
        return Err(format!(
            "expected {PROBES} probe-read rows, found {}; the subject or the trace changed",
            rows.len()
        ));
    }
    if rows.iter().any(|row| !row.complete) {
        return Err(format!("a classifier call did not complete: {rows:?}"));
    }
    let reads: u64 = rows.iter().map(|row| row.reads).sum();
    if reads < READS_FLOOR {
        return Err(format!(
            "{reads} indexed reads is below the {READS_FLOOR} floor; the trace is not counting \
             this run"
        ));
    }
    if reads > READS_CEILING {
        return Err(format!(
            "classifier work grew to {reads} indexed reads for {PROBES} probes, over its \
             {READS_CEILING} budget"
        ));
    }
    Ok(reads)
}

#[test]
fn five_gap_check_stays_inside_its_classifier_read_budget() -> TestResult {
    let (project, _cleanup) = workspace("check-work-budget")?;
    let output = run_command_with_env(
        env!("CARGO_BIN_EXE_ripr"),
        &project,
        &["check", "--diff", "change.diff", "--format", "json"],
        &[(TRACE_ENV, "1"), ("RAYON_NUM_THREADS", "1")],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "ripr check: {stderr}");
    let rows = rows_from_stderr(&stderr)?;
    let reads = judge(&rows)?;
    eprintln!(
        "check-work-budget five-gap check: {reads} indexed reads over {PROBES} probes \
         (ceiling {READS_CEILING}, floor {READS_FLOOR})"
    );
    Ok(())
}

fn row(reads: u64, returned: bool, interrupted: bool) -> String {
    format!(
        "{ROW_PREFIX}{{\"observed_read_events\":{reads},\"dropped_read_events\":3,\
         \"classification_returned\":{returned},\"interrupted\":{interrupted}}}\n"
    )
}

#[test]
fn work_row_reader_reads_every_row_and_refuses_a_malformed_one() -> TestResult {
    let stderr = format!(
        "ripr progress: analyzing\n{}other noise\n{}",
        row(120, true, false),
        row(7, true, true)
    );
    assert_eq!(
        rows_from_stderr(&stderr)?,
        [
            Row {
                reads: 120,
                complete: true
            },
            Row {
                reads: 7,
                complete: false
            },
        ]
    );
    assert_eq!(rows_from_stderr("no trace here\n")?, []);

    let refusals = [
        (format!("{ROW_PREFIX}{{\n"), "not JSON"),
        (
            format!("{ROW_PREFIX}{{\"classification_returned\":true}}\n"),
            "no unsigned `observed_read_events`",
        ),
        (
            format!("{ROW_PREFIX}{{\"observed_read_events\":-1}}\n"),
            "no unsigned `observed_read_events`",
        ),
        (
            format!("{ROW_PREFIX}{{\"observed_read_events\":1}}\n"),
            "no boolean `classification_returned`",
        ),
        (
            format!(
                "{ROW_PREFIX}{{\"observed_read_events\":1,\"classification_returned\":true}}\n"
            ),
            "no boolean `interrupted`",
        ),
    ];
    for (stderr, expected) in refusals {
        match rows_from_stderr(&stderr) {
            Ok(rows) => return Err(format!("{stderr:?} read as {rows:?}").into()),
            Err(reason) => assert!(reason.contains(expected), "{reason}"),
        }
    }
    Ok(())
}

#[test]
fn work_judge_fails_each_bound_and_an_incomplete_measurement() -> TestResult {
    let ok = |reads| Row {
        reads,
        complete: true,
    };
    let even = |total: u64| -> Vec<Row> {
        let each = total / PROBES as u64;
        let mut rows = vec![ok(each); PROBES];
        rows[0].reads += total - each * PROBES as u64;
        rows
    };

    assert_eq!(judge(&even(996)), Ok(996));
    assert_eq!(judge(&even(READS_CEILING)), Ok(READS_CEILING));
    assert_eq!(judge(&even(READS_FLOOR)), Ok(READS_FLOOR));

    let mut incomplete = even(996);
    incomplete[5].complete = false;
    // No single probe is unusual; the sum is over.
    let failures = [
        (even(READS_CEILING + 1), "over its"),
        (even(READS_FLOOR - 1), "not counting"),
        (
            even(996)[1..].to_vec(),
            "expected 8 probe-read rows, found 7",
        ),
        (Vec::new(), "expected 8 probe-read rows, found 0"),
        (incomplete, "did not complete"),
    ];
    for (rows, expected) in failures {
        match judge(&rows) {
            Ok(reads) => {
                return Err(format!("{rows:?} passed at {reads}, expected `{expected}`").into());
            }
            Err(reason) => assert!(reason.contains(expected), "{rows:?}: {reason}"),
        }
    }
    Ok(())
}
