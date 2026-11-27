//! Workflow contract suites share a bounded test executable (#12914).
//!
//! Each suite retains its own module, platform gate and scratch namespace.
//! No suite changes the parent process environment or working directory.

mod workflow_contracts {
    mod hosted_cache_continuity;
    mod inventory_contract;
    mod runner_inventory_diagnostics;
    mod unlocked_install;
    mod wheel_trap_markers;
    mod workflow_action_pins;
}
