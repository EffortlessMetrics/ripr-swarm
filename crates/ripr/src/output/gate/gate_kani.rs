//! Kani proofs for the gate decision status and its fail-closed behavior.
//!
//! Run: `cargo kani -p ripr --harness <name>`.

use super::presentation::status_fails_gate;
use super::{
    GATE_STATUS_BLOCKED, GATE_STATUS_CONFIG_ERROR, GateMode, GateSummary, top_level_status,
};

fn any_mode() -> GateMode {
    let which: u8 = kani::any();
    kani::assume(which < 4); // exactly the four GateMode variants
    match which {
        0 => GateMode::VisibleOnly,
        1 => GateMode::Acknowledgeable,
        2 => GateMode::BaselineCheck,
        _ => GateMode::CalibratedGate,
    }
}

/// Bound: 0 or 1 entries (production reads only emptiness).
fn any_zero_or_one_entry() -> Vec<String> {
    if kani::any() {
        vec![String::new()]
    } else {
        Vec::new()
    }
}

fn any_summary() -> GateSummary {
    GateSummary {
        evaluated: kani::any(),
        blocking: kani::any(),
        acknowledged: kani::any(),
        advisory: kani::any(),
        suppressed: kani::any(),
        not_applicable: kani::any(),
        unknown_confidence: kani::any(),
    }
}

/// The gate never reports pass (or any non-failing status) when a blocking
/// decision, an exception-policy blocker, or a config error is present, and the
/// failing set is exactly {blocked, config_error}.
/// Bounds: warnings/config_errors have 0 or 1 entries; counters are symbolic.
#[kani::proof]
#[kani::unwind(16)] // longest status token is 12 bytes; bounds str comparison loops
fn top_level_status_fails_closed() {
    let summary = any_summary();
    let warnings = any_zero_or_one_entry();
    let config_errors = any_zero_or_one_entry();
    let mode = any_mode();
    let exception_blocking: usize = kani::any();

    let status = top_level_status(
        &summary,
        &warnings,
        &config_errors,
        mode,
        exception_blocking,
    );

    kani::cover!(status == "pass");
    kani::cover!(status == GATE_STATUS_BLOCKED);
    kani::cover!(status == GATE_STATUS_CONFIG_ERROR);
    kani::cover!(status == "acknowledged");
    kani::cover!(status == "advisory");

    // Closed output set.
    assert!(matches!(
        status,
        "pass" | "advisory" | "acknowledged" | GATE_STATUS_BLOCKED | GATE_STATUS_CONFIG_ERROR
    ));
    // Fail-closed: any blocker or config error means the gate fails.
    let must_fail = !config_errors.is_empty() || summary.blocking > 0 || exception_blocking > 0;
    assert_eq!(status_fails_gate(status), must_fail);
    // Config error dominates blocking.
    if !config_errors.is_empty() {
        assert_eq!(status, GATE_STATUS_CONFIG_ERROR);
    }
    // `pass` only when nothing at all needs attention.
    if status == "pass" {
        assert!(
            summary.blocking == 0
                && exception_blocking == 0
                && summary.acknowledged == 0
                && summary.advisory == 0
                && summary.suppressed == 0
                && summary.unknown_confidence == 0
                && warnings.is_empty()
                && config_errors.is_empty()
                && mode != GateMode::VisibleOnly
        );
    }
}

/// `status_fails_gate` is true for exactly the two failing tokens among the
/// closed status set (so exit mapping 2/3 is reachable only from those).
#[kani::proof]
#[kani::unwind(16)] // longest status token is 12 bytes; bounds str comparison loops
fn status_fails_gate_only_for_blocked_and_config_error() {
    let index: u8 = kani::any();
    kani::assume(index < 5); // the five tokens of GATE_DECISION_KNOWN_STATUSES
    // Literal arms keep each token's length concrete for the solver.
    let (status, expected_fail) = match index {
        0 => ("pass", false),
        1 => ("advisory", false),
        2 => ("acknowledged", false),
        3 => (GATE_STATUS_BLOCKED, true),
        _ => (GATE_STATUS_CONFIG_ERROR, true),
    };
    let fails = status_fails_gate(status);
    kani::cover!(fails);
    kani::cover!(!fails);
    assert_eq!(fails, expected_fail);
}
