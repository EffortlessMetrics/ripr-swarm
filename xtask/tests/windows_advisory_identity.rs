//! Production-command discriminators for #5043. The Cargo-shaped logs are
//! synthetic; they establish parser identity, not native Windows execution.
//! The control inventory is deliberately independent of the implementation.
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const CONTROLS: &[(&str, &str, &str)] = &[
    (
        "unittests src/lib.rs",
        "ripr",
        "process_owner::tests::owner_drop_terminates_a_still_running_child",
    ),
    (
        "unittests src/lib.rs",
        "ripr",
        "process_owner::tests::terminate_tree_kills_pipe_inheriting_descendants",
    ),
    (
        "unittests src/lib.rs",
        "ripr",
        "process_owner::tests::terminate_tree_leaves_unrelated_processes_alive",
    ),
    (
        "unittests src/lib.rs",
        "ripr",
        "process_owner::tests::owner_drop_kills_descendants_after_the_primary_exits",
    ),
    (
        "unittests src/lib.rs",
        "ripr",
        "process_owner::tests::terminate_tree_after_primary_exit_kills_descendants",
    ),
    (
        "unittests src/main.rs",
        "xtask",
        "run::tests::capture_output_with_timeout_terminates_pipe_inheriting_descendants",
    ),
    (
        "unittests src/lib.rs",
        "ripr",
        "lsp::tests::initialize_surfaces_poisoned_client_features_store_as_a_session_failure",
    ),
    (
        "unittests src/lib.rs",
        "ripr",
        "lsp::tests::poisoned_initialize_failure_commit_survives_a_wedged_client_channel",
    ),
    (
        "unittests src/lib.rs",
        "ripr",
        "analysis::seam_cache::tests::corpus_fingerprint_is_none_without_a_content_change_witness",
    ),
    (
        "tests/native_path_roots.rs",
        "native_path_roots",
        "spaced_root_check_and_file_fact_cache_round_trip",
    ),
    (
        "tests/native_path_roots.rs",
        "native_path_roots",
        "unicode_root_check_and_file_fact_cache_round_trip",
    ),
    (
        "tests/native_path_roots.rs",
        "native_path_roots",
        "long_root_check_round_trips_or_names_the_windows_path_limit",
    ),
    (
        "tests/native_path_roots.rs",
        "native_path_roots",
        "powershell_launch_passes_a_quoted_unicode_root_to_ripr",
    ),
    (
        "tests/lsp_lifecycle.rs",
        "lsp_lifecycle",
        "refresh_publishes_diagnostics_under_a_spaced_unicode_root",
    ),
    (
        "tests/lsp_lifecycle.rs",
        "lsp_lifecycle",
        "refresh_under_a_root_beyond_max_path_publishes_or_names_the_windows_path_limit",
    ),
];

const HASH: &str = "1111111111111111";
const NAME: &str = "same_name";

fn target(source: &str, stem: &str, rows: &[(&str, &str, &str)]) -> String {
    let mut log = format!(
        "Running {source} (target/debug/deps/{stem}-{HASH}.exe)\nrunning {} tests\n",
        rows.len()
    );
    for (name, result, _) in rows {
        log.push_str(&format!("test {name} ... {result}\n"));
    }
    let failures = rows
        .iter()
        .filter(|(_, result, _)| *result == "FAILED")
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        log.push_str("failures:\n");
        for (name, _, reason) in &failures {
            log.push_str(&format!("---- {name} stdout ----\nError: {reason}\n\n"));
        }
        log.push_str("failures:\n");
        for (name, _, _) in &failures {
            log.push_str(&format!("    {name}\n"));
        }
    }
    log.push_str(&format!("test result: {}. {} passed; {} failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n", if failures.is_empty() { "ok" } else { "FAILED" }, rows.len()-failures.len(), failures.len()));
    log
}

fn controls(omit: Option<&str>) -> String {
    controls_adjusted(omit, None, None)
}

type ExtraRow<'a> = (&'a str, &'a str, (&'a str, &'a str, &'a str));

fn controls_adjusted(
    omit: Option<&str>,
    extra: Option<ExtraRow<'_>>,
    failing: Option<&str>,
) -> String {
    let mut groups = BTreeMap::<(&str, &str), Vec<(&str, &str, &str)>>::new();
    for &(source, stem, name) in CONTROLS {
        if Some(name) != omit {
            groups.entry((source, stem)).or_default().push((
                name,
                if Some(name) == failing {
                    "FAILED"
                } else {
                    "ok"
                },
                "owning control reason",
            ));
        }
    }
    if let Some((source, stem, row)) = extra {
        groups.entry((source, stem)).or_default().push(row);
    }
    groups
        .into_iter()
        .map(|((source, stem), rows)| target(source, stem, &rows))
        .collect()
}

fn alpha(result: &str) -> String {
    target("tests/alpha.rs", "alpha", &[(NAME, result, "alpha reason")])
}
fn beta(result: &str) -> String {
    target("tests/beta.rs", "beta", &[(NAME, result, "beta reason")])
}

struct TempRoot(PathBuf);
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn invoke(first: &str, second: &str) -> Result<Output, String> {
    invoke_with_missing(first, second, None)
}

fn invoke_with_missing(first: &str, second: &str, missing: Option<&str>) -> Result<Output, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root = TempRoot(std::env::temp_dir().join(format!(
        "ripr-winadv-identity-{}-{nonce}",
        std::process::id()
    )));
    fs::create_dir_all(&root.0).map_err(|e| e.to_string())?;
    for (label, log) in [("run1", first), ("run2", second)] {
        fs::write(root.0.join(format!("{label}.log")), log).map_err(|e| e.to_string())?;
        fs::write(
            root.0.join(format!("{label}.status")),
            if log.contains(" ... FAILED") {
                "101\n"
            } else {
                "0\n"
            },
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(name) = missing {
        fs::remove_file(root.0.join(name)).map_err(|e| e.to_string())?;
    }
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("windows-advisory-summary")
        .arg("--run1")
        .arg(root.0.join("run1.log"))
        .arg("--run1-status")
        .arg(root.0.join("run1.status"))
        .arg("--run2")
        .arg(root.0.join("run2.log"))
        .arg("--run2-status")
        .arg(root.0.join("run2.status"))
        .output()
        .map_err(|e| format!("launch summarizer: {e}"))
}

fn verify(
    first: String,
    second: String,
    exit: i32,
    required: &[&str],
    forbidden: &[&str],
) -> Result<(), String> {
    let output = invoke(&first, &second)?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.code() != Some(exit) {
        return Err(format!(
            "expected exit {exit}, got {:?}: {text}",
            output.status.code()
        ));
    }
    for needle in required {
        if !text.contains(needle) {
            return Err(format!("missing {needle:?}: {text}"));
        }
    }
    for needle in forbidden {
        if text.contains(needle) {
            return Err(format!("unexpected {needle:?}: {text}"));
        }
    }
    Ok(())
}

#[test]
fn identical_names_in_distinct_targets_count_twice() -> Result<(), String> {
    let log = controls(None) + &alpha("ok") + &beta("ok");
    verify(log.clone(), log, 0, &["observed 17 pass, 0 fail"], &[])
}
#[test]
fn a_different_target_pass_cannot_mask_absence() -> Result<(), String> {
    verify(
        controls(None) + &alpha("FAILED"),
        controls(None) + &beta("ok"),
        0,
        &["masked_unknown (1)"],
        &["unstable (", "repeated_failure ("],
    )
}
#[test]
fn different_targets_cannot_invent_a_repeated_failure() -> Result<(), String> {
    verify(
        controls(None) + &alpha("FAILED"),
        controls(None) + &beta("FAILED"),
        0,
        &["masked_unknown (2)"],
        &["repeated_failure ("],
    )
}
#[test]
fn swapped_target_outcomes_are_two_unstable_subjects() -> Result<(), String> {
    verify(
        controls(None) + &alpha("FAILED") + &beta("ok"),
        controls(None) + &alpha("ok") + &beta("FAILED"),
        0,
        &["unstable (2)"],
        &["repeated_failure ("],
    )
}
#[test]
fn both_failure_reasons_keep_their_target_and_count() -> Result<(), String> {
    let log = controls(None) + &alpha("FAILED") + &beta("FAILED");
    verify(
        log.clone(),
        log,
        0,
        &[
            "observed 15 pass, 2 fail",
            "repeated_failure (2)",
            "- `tests/alpha.rs (alpha-1111111111111111) :: same_name`\n  - Run 1: Error: alpha reason\n  - Run 2: Error: alpha reason",
            "- `tests/beta.rs (beta-1111111111111111) :: same_name`\n  - Run 1: Error: beta reason\n  - Run 2: Error: beta reason",
        ],
        &[],
    )
}
#[test]
fn the_same_artifact_failure_is_repeated() -> Result<(), String> {
    let log = controls(None) + &alpha("FAILED");
    verify(
        log.clone(),
        log,
        0,
        &["repeated_failure (1)"],
        &["masked_unknown (", "unstable ("],
    )
}
#[test]
fn genuine_absence_remains_unknown() -> Result<(), String> {
    verify(
        controls(None) + &alpha("FAILED"),
        controls(None) + &target("tests/beta.rs", "beta", &[("different_name", "ok", "")]),
        0,
        &["masked_unknown (1)"],
        &["unstable ("],
    )
}
#[test]
fn changed_artifact_hash_cannot_borrow_a_pass() -> Result<(), String> {
    verify(
        controls(None) + &alpha("FAILED"),
        controls(None) + &alpha("ok").replace(HASH, "2222222222222222"),
        0,
        &["masked_unknown (1)"],
        &["unstable ("],
    )
}
#[test]
fn artifact_paths_and_windows_suffix_are_presentation() -> Result<(), String> {
    let first = controls(None) + &alpha("FAILED");
    let second = controls(None)
        + &alpha("ok")
            .replace("target/debug/deps/", "elsewhere/")
            .replace(".exe)", ")");
    verify(first, second, 0, &["unstable (1)"], &["masked_unknown ("])
}
#[test]
fn ansi_and_windows_separators_keep_the_same_artifact() -> Result<(), String> {
    let first = controls(None) + &alpha("FAILED");
    let second = (controls(None) + &alpha("ok"))
        .replace('/', "\\")
        .replace("Running ", "\u{1b}[92mRunning\u{1b}[0m ");
    verify(first, second, 0, &["unstable (1)"], &["masked_unknown ("])
}
#[test]
fn same_source_different_executables_remain_distinct() -> Result<(), String> {
    let first = controls(None)
        + &target(
            "unittests src/main.rs",
            "ripr",
            &[(NAME, "FAILED", "ripr main")],
        );
    let second = controls_adjusted(
        None,
        Some(("unittests src/main.rs", "xtask", (NAME, "ok", ""))),
        None,
    );
    verify(first, second, 0, &["masked_unknown (1)"], &["unstable ("])
}
#[test]
fn missing_release_control_is_refused() -> Result<(), String> {
    let log = controls(Some(CONTROLS[0].2));
    verify(
        log.clone(),
        log,
        1,
        &["not_observed", "could not produce trustworthy evidence"],
        &[],
    )
}
#[test]
fn wrong_target_release_control_cannot_supply_evidence() -> Result<(), String> {
    let name = CONTROLS[0].2;
    let log =
        controls(Some(name)) + &target("tests/unrelated.rs", "unrelated", &[(name, "ok", "")]);
    verify(log.clone(), log, 1, &["not_observed"], &[])
}
#[test]
fn duplicate_owner_artifacts_are_ambiguous() -> Result<(), String> {
    let name = CONTROLS[0].2;
    let log = controls(None)
        + &target("unittests src/lib.rs", "ripr", &[(name, "ok", "")])
            .replace(HASH, "2222222222222222");
    verify(log.clone(), log, 1, &["not_observed", "ambiguous"], &[])
}
#[test]
fn owner_hash_drift_across_the_pair_is_ambiguous() -> Result<(), String> {
    verify(
        controls(None),
        controls(None).replace("ripr-1111111111111111", "ripr-2222222222222222"),
        1,
        &["not_observed", "ambiguous"],
        &[],
    )
}
#[test]
fn unowned_rows_and_malformed_transitions_refuse_evidence() -> Result<(), String> {
    let name = CONTROLS[0].2;
    for header in [
        "",
        "Running custom command\n",
        "Running unittests src/lib.rs (target/debug/deps/ripr-not-a-hash.exe)\n",
    ] {
        let log = format!("{header}test {name} ... ok\n{}", controls(Some(name)));
        verify(
            log.clone(),
            log,
            1,
            &["incomplete_evidence", "provenance:"],
            &[],
        )?;
    }
    Ok(())
}
#[test]
fn malformed_transition_never_reuses_the_previous_owner() -> Result<(), String> {
    let name = CONTROLS[0].2;
    let log = controls(Some(name)) + &format!("Running malformed\ntest {name} ... ok\n");
    verify(
        log.clone(),
        log,
        1,
        &["incomplete_evidence", "unrecognized target header"],
        &[],
    )
}
#[test]
fn doctest_transition_cannot_borrow_a_unit_pass() -> Result<(), String> {
    let name = "src/lib.rs - example (line 1)";
    let first = controls(None)
        + &format!(
            "Doc-tests ripr\ntest {name} ... FAILED\ntest result: FAILED. 0 passed; 1 failed\n"
        );
    let second = controls_adjusted(
        None,
        Some(("unittests src/lib.rs", "ripr", (name, "ok", ""))),
        None,
    );
    verify(
        first,
        second,
        0,
        &["masked_unknown (1)", "Doc-tests ripr"],
        &["unstable ("],
    )
}
#[test]
fn explicit_doctest_transitions_preserve_names_with_spaces() -> Result<(), String> {
    let first = controls(None)
        + "Doc-tests ripr\ntest src/lib.rs - example (line 1) ... FAILED\ntest result: FAILED. 0 passed; 1 failed\n";
    let second = controls(None)
        + "Doc-tests ripr\ntest src/lib.rs - example (line 1) ... ok\ntest result: ok. 1 passed; 0 failed\n";
    verify(
        first,
        second,
        0,
        &[
            "unstable (1)",
            "Doc-tests ripr :: src/lib.rs - example (line 1)",
        ],
        &[],
    )
}
#[test]
fn duplicate_doctest_transitions_are_unproven() -> Result<(), String> {
    let doc = "Doc-tests ripr\ntest src/lib.rs - example (line 1) ... ok\ntest result: ok. 1 passed; 0 failed\n";
    let log = controls(None) + doc + doc;
    verify(
        log.clone(),
        log,
        1,
        &[
            "incomplete_evidence",
            "repeated target header: Doc-tests ripr",
        ],
        &[],
    )
}
#[test]
fn a_failing_owning_control_stays_advisory() -> Result<(), String> {
    let log = controls_adjusted(None, None, Some(CONTROLS[0].2));
    verify(
        log.clone(),
        log,
        0,
        &["repeated_failure (1)", "FAILED | FAILED"],
        &["not_observed"],
    )
}

#[test]
fn missing_log_or_status_never_becomes_clean_evidence() -> Result<(), String> {
    let log = controls(None);
    for name in ["run1.log", "run2.log", "run1.status", "run2.status"] {
        let output = invoke_with_missing(&log, &log, Some(name))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if output.status.code() != Some(1)
            || !stdout.contains("**Evidence failure.**")
            || stdout.contains("No test failed in either run.")
        {
            return Err(format!(
                "missing {name}: unexpected status {:?}: {stdout}",
                output.status.code()
            ));
        }
    }
    Ok(())
}

#[test]
fn completed_owner_cannot_lend_identity_to_an_orphan_tail() -> Result<(), String> {
    let required = CONTROLS[5].2;
    let log = controls_adjusted(
        Some(required),
        Some((
            "unittests src/main.rs",
            "xtask",
            ("unrelated_xtask_test", "ok", ""),
        )),
        None,
    ) + &format!(
        "running 1 test\ntest {required} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n"
    );
    verify(
        log.clone(),
        log,
        1,
        &["incomplete_evidence", "without an admitted target"],
        &[],
    )
}

#[test]
fn native_empty_child_summary_preserves_the_parent_harness() -> Result<(), String> {
    let required = CONTROLS[5].2;
    let log = controls(None).replace(&format!("test {required} ... ok"), &format!("running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2528 filtered out; finished in 0.00s\ntest {required} ... ok"));
    verify(
        log.clone(),
        log,
        0,
        &["observed 15 pass, 0 fail"],
        &["incomplete_evidence", "not_observed"],
    )
}

#[test]
fn nested_child_cannot_supply_an_owning_release_control() -> Result<(), String> {
    let required = CONTROLS[5].2;
    let log = controls_adjusted(Some(required), Some(("unittests src/main.rs", "xtask", ("unrelated_xtask_test", "ok", ""))), None)
        .replace("test unrelated_xtask_test ... ok", &format!("running 1 test\ntest {required} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\ntest unrelated_xtask_test ... ok"));
    verify(
        log.clone(),
        log,
        1,
        &["incomplete_evidence", "unowned nested harness"],
        &[],
    )
}

#[test]
fn ordinary_captured_progress_keeps_its_real_failure_reason() -> Result<(), String> {
    let log = controls(None)
        + &alpha("FAILED").replace(
            "Error: alpha reason",
            "Running cleanup for fixture\nDoc-tests are checked separately\nError: alpha reason",
        );
    verify(
        log.clone(),
        log,
        0,
        &[
            "repeated_failure (1)",
            "- `tests/alpha.rs (alpha-1111111111111111) :: same_name`\n  - Run 1: Error: alpha reason\n  - Run 2: Error: alpha reason",
        ],
        &["incomplete_evidence"],
    )
}

#[test]
fn a_captured_header_cannot_authenticate_another_target() -> Result<(), String> {
    let log = controls(None) + &alpha("FAILED").replace("Error: alpha reason", "Running tests/echoed.rs (target/debug/deps/echoed-1111111111111111.exe)\ntest echoed_name ... ok\nError: alpha reason");
    verify(
        log.clone(),
        log,
        1,
        &[
            "incomplete_evidence",
            "target header inside an unterminated failure block",
        ],
        &[],
    )
}
