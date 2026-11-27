//! Crate-wide terminal-safe stderr.
//!
//! These macros shadow `std::eprintln!` for every module
//! declared after `#[macro_use] mod stderr_guard;` in `lib.rs`. Notices and
//! warnings quote repository text (paths, refs, config values, disclosures),
//! so each formatted line passes through `terminal_text::terminal_safe` and a
//! hostile repository cannot drive the terminal through stderr. One owner
//! instead of a wrap at every call site: a new `eprintln!` is safe by default.
//! There is no `eprint!` shadow: the library has no caller, and an unused
//! macro fails `-D unused-macros`; add one with its first use.
//! The progress sink writes `\r` in-place updates to its stderr handle
//! directly. Notices still go through this macro, which asks the active
//! `for_stderr` sink to finish any held-open TTY row first under the same
//! mutex the heartbeat thread uses (#7439). When no sink is installed, the
//! destination is still `std::eprintln!`.

macro_rules! eprintln {
    () => {
        if !$crate::cli::progress::try_emit_notice("") {
            ::std::eprintln!()
        }
    };
    ($($arg:tt)*) => {{
        let __ripr_notice = $crate::terminal_text::terminal_safe(::std::format!($($arg)*));
        if !$crate::cli::progress::try_emit_notice(&__ripr_notice) {
            ::std::eprintln!("{}", __ripr_notice);
        }
    }};
}

/// [`eprintln!`] that never panics: a closed or broken stderr is ignored
/// instead of panicking. For diagnostics written from `Drop`, where a panic
/// during an unwind would abort the process. It still finishes any held-open
/// progress row first, so the line never lands on the in-place row.
macro_rules! eprintln_best_effort {
    ($($arg:tt)*) => {{
        let __ripr_notice = $crate::terminal_text::terminal_safe(::std::format!($($arg)*));
        if !$crate::cli::progress::try_emit_notice(&__ripr_notice) {
            // Unit-test builds go through libtest's output capture. A raw
            // `io::stderr()` handle bypasses it and lands mid-line in the
            // runner's stdout rows (#15207). Capture-aware `eprintln!`
            // cannot fail on a closed pipe because capture owns the sink.
            #[cfg(test)]
            ::std::eprintln!("{}", __ripr_notice);
            #[cfg(not(test))]
            {
                use ::std::io::Write as _;
                let _ = ::std::writeln!(::std::io::stderr(), "{}", __ripr_notice);
            }
        }
    }};
}
