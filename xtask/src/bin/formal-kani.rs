//! Maintained focused entry for the same native Kani lane used by xtask.
//! This builds the actual process owner without the unrelated product facade.
#[cfg(not(target_os = "linux"))]
compile_error!("formal-kani currently supports Linux; use cargo xtask kani on other platforms");

extern crate self as ripr;
#[path = "../formal_process.rs"]
mod formal_process;
pub use formal_process::analysis;
#[expect(
    dead_code,
    reason = "the actual process owner also exposes the rustup auto-install guard used by other production commands"
)]
#[path = "../../../crates/ripr/src/process_owner.rs"]
pub mod process_owner;
use xtask::{formal_evidence, formal_registry};
#[path = "../ansi.rs"]
mod ansi;
#[path = "../kani_evidence.rs"]
mod kani_evidence;
#[path = "../kani_lane.rs"]
mod kani_lane;
#[expect(
    dead_code,
    reason = "shared run module also exposes bounded runners owned by other xtask commands"
)]
#[expect(
    unused_imports,
    reason = "shared run module reexports diagnostics used by other xtask commands"
)]
#[path = "../run.rs"]
mod run;
#[cfg(test)]
#[path = "../test_cwd.rs"]
mod test_cwd;
#[cfg(test)]
pub(crate) use test_cwd::acquire_test_cwd_read_guard;

fn main() {
    if let Err(error) = process_owner::bind_process_lifetime() {
        eprintln!("formal-kani: process custody unavailable: {error}");
        std::process::exit(2);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = kani_lane::run_lane(&args) {
        eprintln!("formal-kani: {error}");
        std::process::exit(1);
    }
}
