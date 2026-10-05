//! Crate-wide terminal-safe stderr.
//!
//! These macros shadow `std::eprintln!` and `std::eprint!` for every module
//! declared after `#[macro_use] mod stderr_guard;` in `lib.rs`. Notices and
//! warnings quote repository text (paths, refs, config values, disclosures),
//! so each formatted line passes through `terminal_text::terminal_safe` and a
//! hostile repository cannot drive the terminal through stderr. One owner
//! instead of a wrap at every call site: a new `eprintln!` is safe by default.
//! The progress sink writes to the stderr handle directly (it needs `\r` for
//! in-place updates) and is not routed through here.

macro_rules! eprintln {
    () => {
        ::std::eprintln!()
    };
    ($($arg:tt)*) => {
        ::std::eprintln!(
            "{}",
            $crate::terminal_text::terminal_safe(::std::format!($($arg)*))
        )
    };
}

macro_rules! eprint {
    ($($arg:tt)*) => {
        ::std::eprint!(
            "{}",
            $crate::terminal_text::terminal_safe(::std::format!($($arg)*))
        )
    };
}
