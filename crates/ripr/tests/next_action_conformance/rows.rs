//! Executable rows for the #6307 conformance denominator (slice 1).

use super::fixture::{
    BOUNDARY_TEST_APPEND, BoundaryFixture, FIXTURE_SEAM, attempt_ids, run_ripr, split_ripr_command,
};
use super::report::{RowReport, RowResult};
use serde_json::Value;

struct RowDraft<'a> {
    row_id: &'a str,
    action_family: &'a str,
    producer_surface: &'a str,
    selected: bool,
    executed: bool,
    result: RowResult,
    reason: &'a str,
    expected_transition: &'a str,
    observed_transition: &'a str,
    action_identity: &'a str,
    fixture_root: &'a str,
}

fn finish_row(draft: RowDraft<'_>) -> RowReport {
    RowReport {
        row_id: draft.row_id.to_string(),
        action_family: draft.action_family.to_string(),
        producer_surface: draft.producer_surface.to_string(),
        selected: draft.selected,
        executed: draft.executed,
        result: draft.result,
        reason: draft.reason.to_string(),
        expected_transition: draft.expected_transition.to_string(),
        observed_transition: draft.observed_transition.to_string(),
        action_identity: draft.action_identity.to_string(),
        fixture_root: draft.fixture_root.to_string(),
        limitations: Vec::new(),
    }
}

macro_rules! row {
    (
        $row_id:expr, $family:expr, $surface:expr, $selected:expr, $executed:expr, $result:expr,
        $reason:expr, $expected:expr, $observed:expr, $action:expr, $root:expr $(,)?
    ) => {
        finish_row(RowDraft {
            row_id: $row_id,
            action_family: $family,
            producer_surface: $surface,
            selected: $selected,
            executed: $executed,
            result: $result,
            reason: $reason,
            expected_transition: $expected,
            observed_transition: $observed,
            action_identity: $action,
            fixture_root: $root,
        })
    };
}

fn parse_json_output(output: &std::process::Output) -> Result<Value, String> {
    if !output.status.success() {
        return Err(format!(
            "command failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "JSON parse: {error}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn follow_printed_from_foreign(
    fixture: &BoundaryFixture,
    printed: &str,
) -> Result<std::process::Output, String> {
    let tokens = split_ripr_command(printed)?;
    let args: Vec<&str> = tokens.iter().skip(1).map(String::as_str).collect();
    run_ripr(fixture.foreign_cwd(), &args)
}

pub fn check_result_to_inspect(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let selected = true;
    let check = match run_ripr(
        fixture.foreign_cwd(),
        &["check", "--root", &root_arg, "--worktree", "--json"],
    ) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "check_result_inspect",
                "check result -> inspect or repair",
                "ripr check --format json",
                selected,
                false,
                RowResult::InstrumentFailure,
                &error,
                "inspect same finding",
                "spawn failed",
                "",
                &root_arg,
            );
        }
    };
    let report = match parse_json_output(&check) {
        Ok(value) => value,
        Err(reason) => {
            return row!(
                "check_result_inspect",
                "check result -> inspect or repair",
                "ripr check --format json",
                selected,
                true,
                RowResult::InstrumentFailure,
                &reason,
                "inspect same finding",
                "invalid check json",
                "",
                &root_arg,
            );
        }
    };
    let action = &report["canonical_next_action"];
    let detail_route = action["stop"]["detail_route"]
        .as_str()
        .unwrap_or_default()
        .trim();
    if detail_route.is_empty() {
        return row!(
            "check_result_inspect",
            "check result -> inspect or repair",
            "ripr check --format json",
            selected,
            true,
            RowResult::HonestLimitation,
            "check carried no followable inspect route on this fixture",
            "inspect same finding",
            "no detail_route",
            &action["action_class"].to_string(),
            &root_arg,
        );
    }
    let finding_id = report["findings"]
        .as_array()
        .and_then(|findings| findings.first())
        .and_then(|finding| finding["id"].as_str())
        .unwrap_or_default();
    let explained = match follow_printed_from_foreign(fixture, detail_route) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "check_result_inspect",
                "check result -> inspect or repair",
                "ripr check --format json",
                selected,
                true,
                RowResult::InstrumentFailure,
                &error,
                "inspect same finding",
                "follow failed",
                detail_route,
                &root_arg,
            );
        }
    };
    if !explained.status.success() {
        return row!(
            "check_result_inspect",
            "check result -> inspect or repair",
            "ripr check --format json",
            selected,
            true,
            RowResult::CommandRefused,
            &String::from_utf8_lossy(&explained.stderr),
            "inspect same finding",
            "explain refused",
            detail_route,
            &root_arg,
        );
    }
    let stdout = String::from_utf8_lossy(&explained.stdout);
    if finding_id.is_empty() || !stdout.contains(&format!("id: {finding_id}")) {
        return row!(
            "check_result_inspect",
            "check result -> inspect or repair",
            "ripr check --format json",
            selected,
            true,
            RowResult::WrongSubject,
            "explain output did not reopen the check finding id",
            "inspect same finding",
            "missing finding id in explain",
            detail_route,
            &root_arg,
        );
    }
    row!(
        "check_result_inspect",
        "check result -> inspect or repair",
        "ripr check --format json",
        selected,
        true,
        RowResult::AdvancedExpectedState,
        "followed detail_route from foreign cwd and reopened the finding",
        "inspect same finding",
        "explain showed finding id",
        detail_route,
        &root_arg,
    )
}

pub fn repair_card_to_repair(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let card = match run_ripr(
        &fixture.root,
        &[
            "agent",
            "card",
            "--root",
            &root_arg,
            "--seam-id",
            FIXTURE_SEAM,
            "--json",
        ],
    ) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "repair_card_repair",
                "RepairCard -> repair / details",
                "ripr agent card --json",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "start repair for seam",
                "spawn failed",
                FIXTURE_SEAM,
                &root_arg,
            );
        }
    };
    let card_json = match parse_json_output(&card) {
        Ok(value) => value,
        Err(reason) => {
            return row!(
                "repair_card_repair",
                "RepairCard -> repair / details",
                "ripr agent card --json",
                true,
                true,
                RowResult::InstrumentFailure,
                &reason,
                "start repair for seam",
                "invalid card json",
                FIXTURE_SEAM,
                &root_arg,
            );
        }
    };
    let canonical = &card_json["canonical_next_action"];
    if canonical["action_class"] != "run_command" {
        return row!(
            "repair_card_repair",
            "RepairCard -> repair / details",
            "ripr agent card --json",
            true,
            true,
            RowResult::HonestLimitation,
            &format!("card canonical class was {}", canonical["action_class"]),
            "start repair for seam",
            "non-run_command",
            FIXTURE_SEAM,
            &root_arg,
        );
    }
    let before = match run_ripr(
        &fixture.root,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            FIXTURE_SEAM,
            "--phase",
            "before",
        ],
    ) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "repair_card_repair",
                "RepairCard -> repair / details",
                "ripr agent card --json",
                true,
                true,
                RowResult::InstrumentFailure,
                &error,
                "start repair for seam",
                "repair spawn failed",
                FIXTURE_SEAM,
                &root_arg,
            );
        }
    };
    if !before.status.success() || attempt_ids(&fixture.root).is_empty() {
        return row!(
            "repair_card_repair",
            "RepairCard -> repair / details",
            "ripr agent card --json",
            true,
            true,
            RowResult::CommandRefused,
            &String::from_utf8_lossy(&before.stderr),
            "start repair for seam",
            "repair did not start attempt",
            FIXTURE_SEAM,
            &root_arg,
        );
    }
    row!(
        "repair_card_repair",
        "RepairCard -> repair / details",
        "ripr agent card --json",
        true,
        true,
        RowResult::AdvancedExpectedState,
        "card run_command advanced to a live repair attempt",
        "awaiting_edit attempt",
        "repair before phase succeeded",
        FIXTURE_SEAM,
        &root_arg,
    )
}

pub fn ripr_repair_active_attempt(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let repair = match run_ripr(&fixture.root, &["repair", "--root", &root_arg]) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "ripr_repair_attempt",
                "ripr repair -> active RepairAttempt",
                "ripr repair",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "one attempt",
                "spawn failed",
                "",
                &root_arg,
            );
        }
    };
    let ids = attempt_ids(&fixture.root);
    if repair.status.success() && ids.len() == 1 {
        return row!(
            "ripr_repair_attempt",
            "ripr repair -> active RepairAttempt",
            "ripr repair",
            true,
            true,
            RowResult::AdvancedExpectedState,
            "facade repair created exactly one attempt",
            "awaiting_edit",
            &format!("attempt {}", ids[0]),
            FIXTURE_SEAM,
            &root_arg,
        );
    }
    row!(
        "ripr_repair_attempt",
        "ripr repair -> active RepairAttempt",
        "ripr repair",
        true,
        true,
        RowResult::CommandRefused,
        &String::from_utf8_lossy(&repair.stderr),
        "one attempt",
        &format!("attempt count {}", ids.len()),
        "",
        &root_arg,
    )
}

pub fn ripr_status_continue(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let _ = run_ripr(&fixture.root, &["repair", "--root", &root_arg]);
    let ids = attempt_ids(&fixture.root);
    let Some(attempt_id) = ids.first() else {
        return row!(
            "ripr_status_continue",
            "ripr status -> continue / choose / restart / terminal details",
            "ripr status --json",
            true,
            true,
            RowResult::InstrumentFailure,
            "no attempt to inspect",
            "continue command",
            "missing attempt",
            "",
            &root_arg,
        );
    };
    let status = match run_ripr(
        &fixture.root,
        &[
            "status",
            "--root",
            &root_arg,
            "--attempt",
            attempt_id,
            "--json",
        ],
    ) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "ripr_status_continue",
                "ripr status -> continue / choose / restart / terminal details",
                "ripr status --json",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "continue command",
                "spawn failed",
                attempt_id,
                &root_arg,
            );
        }
    };
    let report = match parse_json_output(&status) {
        Ok(value) => value,
        Err(reason) => {
            return row!(
                "ripr_status_continue",
                "ripr status -> continue / choose / restart / terminal details",
                "ripr status --json",
                true,
                true,
                RowResult::InstrumentFailure,
                &reason,
                "continue command",
                "invalid status json",
                attempt_id,
                &root_arg,
            );
        }
    };
    let command = report
        .pointer("/next_action/command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if command.contains("ripr continue") {
        return row!(
            "ripr_status_continue",
            "ripr status -> continue / choose / restart / terminal details",
            "ripr status --json",
            true,
            true,
            RowResult::AdvancedExpectedState,
            "status named the ordinary continue route for the live attempt",
            "continue command",
            command,
            attempt_id,
            &root_arg,
        );
    }
    row!(
        "ripr_status_continue",
        "ripr status -> continue / choose / restart / terminal details",
        "ripr status --json",
        true,
        true,
        RowResult::HonestLimitation,
        "status did not offer continue on this state",
        "continue command",
        command,
        attempt_id,
        &root_arg,
    )
}

pub fn ripr_continue_advance(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let repair = match run_ripr(&fixture.root, &["repair", "--root", &root_arg]) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "ripr_continue_advance",
                "ripr continue -> terminal or next governed state",
                "ripr continue",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "ready_to_finish",
                "spawn failed",
                "",
                &root_arg,
            );
        }
    };
    if !repair.status.success() {
        return row!(
            "ripr_continue_advance",
            "ripr continue -> terminal or next governed state",
            "ripr continue",
            true,
            true,
            RowResult::CommandRefused,
            &String::from_utf8_lossy(&repair.stderr),
            "ready_to_finish",
            "repair refused",
            "",
            &root_arg,
        );
    }
    let attempt_id = attempt_ids(&fixture.root)
        .into_iter()
        .next()
        .unwrap_or_default();
    let pricing =
        std::fs::read_to_string(fixture.root.join("tests/pricing.rs")).unwrap_or_default();
    if std::fs::write(
        fixture.root.join("tests/pricing.rs"),
        format!("{pricing}{BOUNDARY_TEST_APPEND}"),
    )
    .is_err()
    {
        return row!(
            "ripr_continue_advance",
            "ripr continue -> terminal or next governed state",
            "ripr continue",
            true,
            true,
            RowResult::InstrumentFailure,
            "could not write focused edit",
            "ready_to_finish",
            "edit failed",
            &attempt_id,
            &root_arg,
        );
    }
    let continued = match run_ripr(&fixture.root, &["continue", "--root", &root_arg]) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "ripr_continue_advance",
                "ripr continue -> terminal or next governed state",
                "ripr continue",
                true,
                true,
                RowResult::InstrumentFailure,
                &error,
                "ready_to_finish",
                "continue spawn failed",
                &attempt_id,
                &root_arg,
            );
        }
    };
    if continued.status.success() {
        return row!(
            "ripr_continue_advance",
            "ripr continue -> terminal or next governed state",
            "ripr continue",
            true,
            true,
            RowResult::AdvancedExpectedState,
            "continue advanced the sole current attempt after the focused edit",
            "ready_to_finish",
            "continue exit 0",
            &attempt_id,
            &root_arg,
        );
    }
    row!(
        "ripr_continue_advance",
        "ripr continue -> terminal or next governed state",
        "ripr continue",
        true,
        true,
        RowResult::PrerequisiteRequired,
        &String::from_utf8_lossy(&continued.stderr),
        "ready_to_finish",
        "continue non-zero",
        &attempt_id,
        &root_arg,
    )
}

pub fn doctor_recovery_action(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let _ = std::fs::write(fixture.root.join("ripr.toml"), "this is = not [valid toml");
    let doctor = match run_ripr(&fixture.root, &["doctor", "--root", &root_arg, "--json"]) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "doctor_recovery",
                "doctor -> precise setup or recovery action",
                "ripr doctor --json",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "config recovery stop",
                "spawn failed",
                "",
                &root_arg,
            );
        }
    };
    if doctor.stdout.is_empty() {
        return row!(
            "doctor_recovery",
            "doctor -> precise setup or recovery action",
            "ripr doctor --json",
            true,
            true,
            RowResult::InstrumentFailure,
            &String::from_utf8_lossy(&doctor.stderr),
            "config recovery stop",
            "doctor produced no json stdout",
            "",
            &root_arg,
        );
    }
    let doc = match serde_json::from_slice::<Value>(&doctor.stdout) {
        Ok(value) => value,
        Err(reason) => {
            return row!(
                "doctor_recovery",
                "doctor -> precise setup or recovery action",
                "ripr doctor --json",
                true,
                true,
                RowResult::InstrumentFailure,
                &format!("{reason}\n{}", String::from_utf8_lossy(&doctor.stdout)),
                "config recovery stop",
                "invalid doctor json",
                "",
                &root_arg,
            );
        }
    };
    let action = doc.get("canonical_next_action");
    if action.is_some()
        && action
            .and_then(|value| value.get("stop"))
            .and_then(|stop| stop.get("kind"))
            .and_then(|kind| kind.as_str())
            == Some("doctor_recovery")
    {
        return row!(
            "doctor_recovery",
            "doctor -> precise setup or recovery action",
            "ripr doctor --json",
            true,
            true,
            RowResult::HonestLimitation,
            "doctor named typed config recovery without inventing a runnable check route",
            "doctor_recovery stop",
            "canonical_next_action present",
            "config",
            &root_arg,
        );
    }
    row!(
        "doctor_recovery",
        "doctor -> precise setup or recovery action",
        "ripr doctor --json",
        true,
        true,
        RowResult::WrongSubject,
        "broken config must surface doctor_recovery canonical action",
        "doctor_recovery stop",
        "missing or wrong stop",
        "",
        &root_arg,
    )
}

pub fn pilot_delegated_action(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let pilot = match run_ripr(
        &fixture.root,
        &["pilot", "--root", &root_arg, "--quiet", "--max-seams", "1"],
    ) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "pilot_delegated",
                "pilot -> delegated check/repair/inspect action",
                "ripr pilot",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "delegated transaction",
                "spawn failed",
                "",
                &root_arg,
            );
        }
    };
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&pilot.stdout),
        String::from_utf8_lossy(&pilot.stderr)
    );
    if combined.contains("ripr check") || combined.contains("ripr repair") {
        return row!(
            "pilot_delegated",
            "pilot -> delegated check/repair/inspect action",
            "ripr pilot",
            true,
            true,
            RowResult::AdvancedExpectedState,
            "pilot delegated to an ordinary check or repair route on the boundary fixture",
            "pilot transaction",
            "delegated route text present",
            "pilot",
            &root_arg,
        );
    }
    row!(
        "pilot_delegated",
        "pilot -> delegated check/repair/inspect action",
        "ripr pilot",
        true,
        true,
        RowResult::HonestLimitation,
        "pilot ended without a copyable delegated route on this fixture",
        "pilot transaction",
        &combined,
        "pilot",
        &root_arg,
    )
}

pub fn control_wrong_root_decoy(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let decoy_arg = fixture.foreign_cwd().to_string_lossy().into_owned();
    let output = match run_ripr(
        fixture.foreign_cwd(),
        &["check", "--root", &decoy_arg, "--json"],
    ) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "control_wrong_root",
                "control: foreign root",
                "ripr check --json",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "no authoritative read of fixture",
                "spawn failed",
                &decoy_arg,
                &root_arg,
            );
        }
    };
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        if text.contains(FIXTURE_SEAM) || text.contains("discounted_total") {
            return row!(
                "control_wrong_root",
                "control: foreign root",
                "ripr check --json",
                true,
                true,
                RowResult::WrongSubject,
                "decoy root must not observe the boundary fixture finding",
                "refusal or empty unrelated repo",
                "leaked fixture subject into decoy root",
                &decoy_arg,
                &root_arg,
            );
        }
        return row!(
            "control_wrong_root",
            "control: foreign root",
            "ripr check --json",
            true,
            true,
            RowResult::AdvancedExpectedState,
            "empty decoy root did not inherit committed fixture analysis",
            "isolated decoy root",
            "check succeeded without fixture ids",
            &decoy_arg,
            &root_arg,
        );
    }
    row!(
        "control_wrong_root",
        "control: foreign root",
        "ripr check --json",
        true,
        true,
        RowResult::CommandRefused,
        &String::from_utf8_lossy(&output.stderr),
        "isolated decoy root",
        "check refused on empty decoy",
        &decoy_arg,
        &root_arg,
    )
}

pub fn control_committed_vs_worktree(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let _ = std::fs::write(
        fixture.root.join("src/lib.rs"),
        "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount > discount_threshold {\n        amount - 20\n    } else {\n        amount\n    }\n}\n",
    );
    let committed = run_ripr(
        &fixture.root,
        &["check", "--root", &root_arg, "--committed", "--json"],
    );
    let worktree = run_ripr(
        &fixture.root,
        &["check", "--root", &root_arg, "--worktree", "--json"],
    );
    let (Ok(committed), Ok(worktree)) = (committed, worktree) else {
        return row!(
            "control_diff_mode",
            "control: diff mode",
            "ripr check --json",
            true,
            true,
            RowResult::InstrumentFailure,
            "could not run committed/worktree checks",
            "distinct diff_source actions",
            "spawn failed",
            "",
            &root_arg,
        );
    };
    let committed_json = parse_json_output(&committed).unwrap_or(Value::Null);
    let worktree_json = parse_json_output(&worktree).unwrap_or(Value::Null);
    let committed_source = committed_json["canonical_next_action"]["subject"]["diff_source"]
        .as_object()
        .and_then(|map| map.keys().next().map(|k| k.to_string()))
        .unwrap_or_default();
    let worktree_source = worktree_json["canonical_next_action"]["subject"]["diff_source"]
        .as_object()
        .and_then(|map| map.keys().next().map(|k| k.to_string()))
        .unwrap_or_default();
    if committed_source == "committed"
        && worktree_source == "working_tree"
        && committed_json["findings"] != worktree_json["findings"]
    {
        return row!(
            "control_diff_mode",
            "control: diff mode",
            "ripr check --json",
            true,
            true,
            RowResult::AdvancedExpectedState,
            "committed and working-tree modes stayed distinct on a dirty fixture",
            "distinct diff_source and findings",
            "modes differ",
            "",
            &root_arg,
        );
    }
    let observed = format!("committed={committed_source}, worktree={worktree_source}");
    row!(
        "control_diff_mode",
        "control: diff mode",
        "ripr check --json",
        true,
        true,
        RowResult::StaleAction,
        "committed vs worktree did not diverge as expected on dirty fixture",
        "distinct diff_source",
        &observed,
        "",
        &root_arg,
    )
}

pub fn cli_projection_parity(fixture: &BoundaryFixture) -> RowReport {
    let root_arg = fixture.root_arg();
    let json = match run_ripr(
        &fixture.root,
        &["check", "--root", &root_arg, "--worktree", "--json"],
    ) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "cli_projection_parity",
                "projection parity (CLI human/json)",
                "ripr check",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "same inspect route",
                "spawn failed",
                "",
                &root_arg,
            );
        }
    };
    let human = match run_ripr(&fixture.root, &["check", "--root", &root_arg, "--worktree"]) {
        Ok(output) => output,
        Err(error) => {
            return row!(
                "cli_projection_parity",
                "projection parity (CLI human/json)",
                "ripr check",
                true,
                false,
                RowResult::InstrumentFailure,
                &error,
                "same inspect route",
                "human spawn failed",
                "",
                &root_arg,
            );
        }
    };
    let report = match parse_json_output(&json) {
        Ok(value) => value,
        Err(reason) => {
            return row!(
                "cli_projection_parity",
                "projection parity (CLI human/json)",
                "ripr check",
                true,
                true,
                RowResult::InstrumentFailure,
                &reason,
                "same inspect route",
                "invalid json",
                "",
                &root_arg,
            );
        }
    };
    if !human.status.success() {
        return row!(
            "cli_projection_parity",
            "projection parity (CLI human/json)",
            "ripr check",
            true,
            true,
            RowResult::CommandRefused,
            &String::from_utf8_lossy(&human.stderr),
            "same inspect route",
            "human check failed",
            "",
            &root_arg,
        );
    }
    let route = report["canonical_next_action"]["stop"]["detail_route"]
        .as_str()
        .unwrap_or_default()
        .trim();
    let human_out = String::from_utf8_lossy(&human.stdout);
    let printed_explain = human_out
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("ripr explain "));
    if !route.is_empty()
        && let Some(line) = printed_explain
        && line == route
    {
        return row!(
            "cli_projection_parity",
            "projection parity (CLI human/json)",
            "ripr check",
            true,
            true,
            RowResult::AdvancedExpectedState,
            "human listing carried the same inspect route as check JSON",
            "matching detail_route",
            line,
            "",
            &root_arg,
        );
    }
    row!(
        "cli_projection_parity",
        "projection parity (CLI human/json)",
        "ripr check",
        true,
        true,
        RowResult::StaleAction,
        "human and JSON inspect routes diverged",
        "matching detail_route",
        route,
        "",
        &root_arg,
    )
}
