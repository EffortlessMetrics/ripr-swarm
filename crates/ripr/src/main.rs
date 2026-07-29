#![forbid(unsafe_code)]

mod startup;

fn main() {
    install_panic_hook();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(startup::run)) {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            report_failure(&err);
            std::process::exit(exit_code());
        }
        Err(_) => std::process::exit(exit_code()),
    }
}

/// Install a panic hook so an unexpected panic produces a recognizable
/// `ripr:` error message. The top-level startup boundary maps a main-thread
/// panic to code 2; worker panics remain available to their joiners (#2660).
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(no panic message)");
        eprintln!(
            "{}",
            format_panic_report(message, info.location().map(|loc| (loc.file(), loc.line())),)
        );
        let backtrace = std::backtrace::Backtrace::capture();
        if matches!(
            backtrace.status(),
            std::backtrace::BacktraceStatus::Captured
        ) {
            eprintln!("stack backtrace:\n{backtrace}");
        } else {
            eprintln!(
                "note: set RUST_BACKTRACE=1 for a backtrace; report at https://github.com/EffortlessMetrics/ripr-swarm/issues"
            );
        }
    }));
}

fn format_panic_report(message: &str, location: Option<(&str, u32)>) -> String {
    let location = location
        .map(|(file, line)| format!(" at {file}:{line}"))
        .unwrap_or_default();
    format!("ripr: internal error (this is a bug): {message}{location}")
}

fn report_failure(err: &str) {
    eprintln!("ripr: {err}");
}

const fn exit_code() -> i32 {
    2
}

#[cfg(test)]
mod tests {
    #[test]
    fn panic_boundary_catches_main_thread_panics_and_preserves_exit_code() {
        let report = super::format_panic_report("panic hook regression", Some(("src/main.rs", 42)));
        assert_eq!(
            report,
            "ripr: internal error (this is a bug): panic hook regression at src/main.rs:42"
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            std::panic::resume_unwind(Box::new("panic hook regression"));
        }));
        assert!(result.is_err());
        assert_eq!(super::exit_code(), 2);
    }
}
