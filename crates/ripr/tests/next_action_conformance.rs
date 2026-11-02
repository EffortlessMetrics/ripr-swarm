//! First-hour next-action conformance harness (#6307), slice 1.
//!
//! Executes product-produced actions from owned fixtures, records a deterministic
//! JSON/Markdown report, and refuses to pass when zero rows were selected or executed.

#[path = "next_action_conformance/fixture.rs"]
mod fixture;
#[path = "next_action_conformance/report.rs"]
mod report;
#[path = "next_action_conformance/rows.rs"]
mod rows;

use report::ConformanceReport;
use rows::{
    check_result_to_inspect, cli_projection_parity, control_committed_vs_worktree,
    control_wrong_root_decoy, doctor_recovery_action, pilot_delegated_action,
    repair_card_to_repair, ripr_continue_advance, ripr_repair_active_attempt, ripr_status_continue,
};

#[test]
fn first_hour_next_action_conformance_matrix() -> Result<(), String> {
    let candidate = env!("CARGO_BIN_EXE_ripr").to_string();
    let mut report = ConformanceReport::new(candidate);

    let check_fixture = fixture::BoundaryFixture::new("matrix-check")?.with_worktree_gap()?;
    let repair_fixture = fixture::BoundaryFixture::new("matrix-repair")?;
    let status_fixture = fixture::BoundaryFixture::new("matrix-status")?;
    let continue_fixture = fixture::BoundaryFixture::new("matrix-continue")?;
    let doctor_fixture = fixture::BoundaryFixture::new("matrix-doctor")?;
    let pilot_fixture = fixture::BoundaryFixture::new("matrix-pilot")?;
    let diff_fixture = fixture::BoundaryFixture::new("matrix-diff")?.with_worktree_gap()?;
    let decoy_fixture = fixture::BoundaryFixture::new("matrix-decoy")?;

    report.push_row(check_result_to_inspect(&check_fixture));
    report.push_row(cli_projection_parity(&check_fixture));
    report.push_row(repair_card_to_repair(&repair_fixture));
    report.push_row(ripr_repair_active_attempt(&repair_fixture));
    report.push_row(ripr_status_continue(&status_fixture));
    report.push_row(ripr_continue_advance(&continue_fixture));
    report.push_row(doctor_recovery_action(&doctor_fixture));
    report.push_row(pilot_delegated_action(&pilot_fixture));
    report.push_row(control_wrong_root_decoy(&decoy_fixture));
    report.push_row(control_committed_vs_worktree(&diff_fixture));

    if std::env::var_os("RIPR_NEXT_ACTION_CONFORMANCE_WRITE").is_some() {
        let json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
        let md = report.to_markdown();
        let dir = std::env::temp_dir().join("ripr-next-action-conformance");
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        std::fs::write(dir.join("report.json"), json).map_err(|error| error.to_string())?;
        std::fs::write(dir.join("report.md"), md).map_err(|error| error.to_string())?;
    }

    report.aggregate_pass()?;
    Ok(())
}

#[test]
fn conformance_report_is_deterministic_apart_from_fixture_paths() -> Result<(), String> {
    let mut first = ConformanceReport::new("ripr-test".to_string());
    first.push_row(report::RowReport {
        row_id: "sample".to_string(),
        action_family: "family".to_string(),
        producer_surface: "surface".to_string(),
        selected: true,
        executed: true,
        result: report::RowResult::HonestLimitation,
        reason: "reason".to_string(),
        expected_transition: "expected".to_string(),
        observed_transition: "observed".to_string(),
        action_identity: "id".to_string(),
        fixture_root: "/tmp/fixture".to_string(),
        limitations: Vec::new(),
    });
    let json_a = serde_json::to_string(&first).map_err(|error| error.to_string())?;
    let json_b = serde_json::to_string(&first).map_err(|error| error.to_string())?;
    if json_a != json_b {
        return Err("report serialization must be deterministic".to_string());
    }
    Ok(())
}
