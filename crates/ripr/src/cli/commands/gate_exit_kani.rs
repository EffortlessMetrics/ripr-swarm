//! Kani proof for the `gate evaluate` status-to-exit mapping.
//!
//! Run: `cargo kani -p ripr --harness <name>`.

use super::gate::gate_evaluate_exit_error;
use crate::output::gate::{GATE_STATUS_BLOCKED, GATE_STATUS_CONFIG_ERROR};

/// `gate evaluate` failing statuses: `config_error` is could-not-complete (2);
/// every other failing status is a completed blocking decision (3). Never 0.
/// Bound: status ranges over the closed five-token schema set, selected by a
/// symbolic index with literal arms so each token's length stays concrete.
#[kani::proof]
#[kani::unwind(16)] // longest status token is 12 bytes; bounds str comparison loops
fn gate_evaluate_exit_error_maps_status_to_documented_code() {
    let index: u8 = kani::any();
    kani::assume(index < 5);
    let (status, is_config_error) = match index {
        0 => ("pass", false),
        1 => ("advisory", false),
        2 => ("acknowledged", false),
        3 => (GATE_STATUS_BLOCKED, false),
        _ => (GATE_STATUS_CONFIG_ERROR, true),
    };
    let code = gate_evaluate_exit_error(status, String::new()).exit_code();
    kani::cover!(is_config_error && code == 2);
    kani::cover!(index == 3 && code == 3);
    assert!(code == 2 || code == 3);
    if is_config_error {
        assert_eq!(code, 2);
    } else {
        assert_eq!(code, 3);
    }
}
