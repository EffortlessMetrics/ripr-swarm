//! Focused compilation of the real formal lane and shared process owner.
//! Automation mode uses the binary's real ripr dependency. Narrow Linux mode
//! compiles its actual process-owner source and exact cancellation contract
//! with cfg(test), avoiding the unrelated analyzer facade. Neither mode
//! substitutes a capture, deadline, CWD gate or ANSI parser implementation.

#[cfg(all(not(feature = "automation"), not(target_os = "linux")))]
compile_error!("narrow formal-runner controls require Linux; use automation for other platforms");

#[cfg(not(feature = "automation"))]
extern crate self as ripr;
#[cfg(not(feature = "automation"))]
#[path = "../src/formal_process.rs"]
mod formal_process;
#[cfg(not(feature = "automation"))]
pub use formal_process::analysis;
#[cfg(not(feature = "automation"))]
#[expect(
    dead_code,
    reason = "the actual owner also defines the rustup auto-install guard used by other production commands"
)]
#[path = "../../crates/ripr/src/process_owner.rs"]
pub mod process_owner;

#[cfg(all(not(feature = "automation"), target_os = "linux"))]
#[path = "kani_version_probe.rs"]
mod probe_controls;

#[path = "../src/ansi.rs"]
mod ansi;

#[cfg_attr(
    feature = "automation",
    expect(
        dead_code,
        reason = "automation mode exercises the lane entry through the separate full-CLI target"
    )
)]
#[path = "../src/kani_lane.rs"]
mod kani_lane;

#[expect(
    dead_code,
    reason = "the actual process owner exposes additional runners used by other xtask commands"
)]
#[expect(
    unused_imports,
    reason = "the process owner reexports diagnostic disclosure for production callers outside this focused target"
)]
#[path = "../src/run.rs"]
mod run;

#[expect(
    dead_code,
    reason = "the shared CWD gate retains its writer side for other binary tests; these owner tests use readers"
)]
#[path = "../src/test_cwd.rs"]
mod test_cwd;
pub(crate) use test_cwd::acquire_test_cwd_read_guard;
