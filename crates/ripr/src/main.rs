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
        std::process::exit(exit_code());
    }));
}

fn report_failure(err: &str) {
    eprintln!("ripr: {err}");
}

const fn exit_code() -> i32 {
    2
}

#[cfg(test)]
mod tests {
    const PANIC_HOOK_CHILD_ENV: &str = "RIPR_PANIC_HOOK_CHILD";

    #[test]
    fn panic_hook_reports_ripr_error_and_exit_code() -> Result<(), Box<dyn std::error::Error>> {
        if std::env::var_os(PANIC_HOOK_CHILD_ENV).is_some() {
            return Ok(());
        }

        let output = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "tests::panic_hook_child_process", "--nocapture"])
            .env(PANIC_HOOK_CHILD_ENV, "1")
            .env("RUST_BACKTRACE", "0")
            .output()?;
        assert_eq!(output.status.code(), Some(super::exit_code()));
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("ripr: internal error (this is a bug):"));
        assert!(stderr.contains("panic hook regression"));
        assert!(stderr.contains("note: set RUST_BACKTRACE=1 for a backtrace"));
        assert!(!stderr.contains("thread '"));
        Ok(())
    }

    #[test]
    fn panic_hook_child_process() {
        if std::env::var_os(PANIC_HOOK_CHILD_ENV).as_deref() != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        super::install_panic_hook();
        let trigger = std::env::var(PANIC_HOOK_CHILD_ENV).unwrap_or_default();
        assert_eq!(trigger, "trigger", "panic hook regression");
    }
}
