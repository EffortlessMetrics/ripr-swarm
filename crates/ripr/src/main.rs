#![forbid(unsafe_code)]

mod startup;

fn main() {
    install_panic_hook();
    if let Err(err) = startup::run() {
        report_failure(&err);
        std::process::exit(exit_code());
    }
}

/// Install a panic hook so an unexpected panic produces a recognizable
/// `ripr:` error message and exits with the documented code 2, not the
/// default Rust panic output with exit code 101 (#2660).
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(no panic message)");
        let location = info
            .location()
            .map(|loc| format!(" at {}:{}", loc.file(), loc.line()))
            .unwrap_or_default();
        eprintln!("ripr: internal error (this is a bug): {message}{location}");
        eprintln!(
            "note: set RUST_BACKTRACE=1 for a backtrace; report at https://github.com/EffortlessMetrics/ripr-swarm/issues"
        );
        std::process::exit(exit_code());
    }));
}

fn report_failure(err: &str) {
    eprintln!("ripr: {err}");
}

const fn exit_code() -> i32 {
    2
}
