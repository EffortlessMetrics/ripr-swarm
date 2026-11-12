//! Subprocess-safe workflow suites share one bounded test executable.
//! The incident-route suite keeps a separate process because it changes cwd.

mod workflow_contracts {
    mod ci_leaf_contract;
    mod docs_only_routing;
    mod hosted_cache_continuity;
    mod inventory_contract;
    mod printed_command_admission;
    mod review_disposition_admission;
    mod runner_inventory_diagnostics;
    mod rust_gates_admission;
    mod rust_gates_admission_replay;
    mod rust_only_routing;
    mod unlocked_install;
    mod wheel_trap_markers;
}
