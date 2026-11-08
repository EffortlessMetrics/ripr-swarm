//! Arg-parsing and dispatch for `ripr lsp`.
//!
//! This is the CLI adapter layer only. LSP protocol/runtime authority remains
//! in `crate::lsp::*`.

use crate::cli::help;
use crate::cli::suggest::unknown_argument;
use std::io::{IsTerminal, Write};

pub(in crate::cli) fn lsp(args: &[String]) -> Result<(), String> {
    lsp_with_serve(args, || {
        write_lsp_terminal_hint(
            std::io::stdin().is_terminal(),
            &mut std::io::stderr().lock(),
        );
        crate::lsp::serve()
    })
}

/// A person who typed `ripr lsp --stdio` at a prompt would otherwise see a
/// silent hang; an editor pipes stdin, so the hint never reaches one. stderr
/// only (stdout carries LSP framing), and best effort so an unwritable stderr
/// cannot keep the server from starting (issue #10347).
fn write_lsp_terminal_hint(stdin_is_terminal: bool, stderr: &mut impl Write) {
    if stdin_is_terminal {
        let _ = writeln!(
            stderr,
            "ripr lsp: waiting for an editor on stdin (this is a language server, not an interactive command); start it from your editor, or press Ctrl-D to exit."
        );
    }
}

/// Argument dispatch for `ripr lsp`, separated from the blocking stdio
/// transport so unit tests can prove accepted-argument/dispatch semantics
/// without entering the real server (issue #1387: the server owns tokio
/// stdin/stdout and waits indefinitely while stdin stays open, so a test
/// that calls `lsp(&["--stdio"])` hangs). Production always passes
/// `crate::lsp::serve`; tests inject a nonblocking stub.
fn lsp_with_serve(
    args: &[String],
    serve: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    for arg in args {
        match arg.as_str() {
            "--stdio" => {}
            "--version" | "-V" => {
                println!("ripr-lsp {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                help::print_lsp_help();
                return Ok(());
            }
            other => return Err(unknown_argument("lsp", other)),
        }
    }
    serve()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn lsp_help_returns_ok() {
        assert_eq!(lsp(&args(&["--help"])), Ok(()));
    }

    #[test]
    fn lsp_version_returns_ok() {
        assert_eq!(lsp(&args(&["--version"])), Ok(()));
    }

    #[test]
    fn lsp_rejects_unknown_arguments() {
        assert_eq!(
            lsp(&args(&["--bad"])),
            Err("unknown lsp argument \"--bad\". Run `ripr lsp --help`.".to_string())
        );
    }

    #[test]
    fn lsp_accepts_stdio_flag() {
        // Must not enter the real blocking stdio server (issue #1387: it
        // owns tokio stdin/stdout and waits indefinitely while stdin stays
        // open). The injected stub proves `--stdio` is accepted and
        // dispatches to the transport.
        let entered = std::cell::Cell::new(false);
        assert_eq!(
            lsp_with_serve(&args(&["--stdio"]), || {
                entered.set(true);
                Ok(())
            }),
            Ok(())
        );
        assert!(
            entered.get(),
            "expected --stdio to dispatch to the transport"
        );
    }

    #[test]
    fn lsp_default_args_enter_transport() {
        let entered = std::cell::Cell::new(false);
        assert_eq!(
            lsp_with_serve(&args(&[]), || {
                entered.set(true);
                Ok(())
            }),
            Ok(())
        );
        assert!(
            entered.get(),
            "expected default args to dispatch to the transport"
        );
    }

    #[test]
    fn lsp_propagates_transport_errors() {
        assert_eq!(
            lsp_with_serve(&args(&["--stdio"]), || Err("transport down".to_string())),
            Err("transport down".to_string())
        );
    }

    #[test]
    fn lsp_terminal_hint_is_written_only_for_terminal_stdin() {
        let mut shown = Vec::new();
        write_lsp_terminal_hint(true, &mut shown);
        let text = String::from_utf8_lossy(&shown);
        assert!(
            text.starts_with("ripr lsp: waiting for an editor on stdin"),
            "{text}"
        );
        assert!(text.contains("Ctrl-D"), "{text}");
        assert_eq!(text.matches('\n').count(), 1, "single line: {text}");

        let mut piped = Vec::new();
        write_lsp_terminal_hint(false, &mut piped);
        assert!(piped.is_empty());
    }

    #[test]
    fn lsp_terminal_hint_survives_an_unwritable_stderr() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        write_lsp_terminal_hint(true, &mut Closed);
    }

    #[test]
    fn lsp_version_returns_ok_with_short_flag() {
        assert_eq!(lsp(&args(&["-V"])), Ok(()));
    }
}
