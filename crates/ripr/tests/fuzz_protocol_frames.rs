//! Panic and hang freedom for hostile protocol input (#11439).
//!
//! Drives `ripr mcp --stdio` (newline-delimited JSON-RPC) and `ripr lsp --stdio`
//! (`Content-Length` framed JSON-RPC) with a valid session whose raw bytes are
//! mutated: corrupted frame headers, truncated bodies, stray quotes and
//! brackets, invalid UTF-8. Stdin is closed after the bytes are written, so a
//! well-behaved server must exit on its own. A signal, a Rust panic exit
//! (101) or a missed deadline is the finding; any other exit is acceptable.
//! The mutation list is the proptest input, so a failure shrinks.
//!
//! `PROPTEST_CASES` raises the case count for the scheduled `fuzz.yml` run.

use proptest::prelude::*;
use proptest::test_runner::Config;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_mins(1);
const DEFAULT_CASES: u32 = 12;
const MAX_BYTES: usize = 200_000;

const TOKENS: &[&[u8]] = &[
    b"\"",
    b"'",
    b"\\",
    b"{",
    b"}",
    b"[",
    b"]",
    b"\n",
    b"\r\n\r\n",
    b"\xff\xfe",
    b"\0",
    b"Content-Length: ",
    b"Content-Length: 99999999999999999999\r\n\r\n",
    b"Content-Length: -1\r\n\r\n",
    b"\"\\ud800\"",
    b"\"jsonrpc\":\"2.0\"",
    b"null",
    b"1e999",
    b"/*",
    b"\"method\":\"exit\"",
];

type Edit = (u8, u32, u8, u16);

fn edits() -> impl Strategy<Value = Vec<Edit>> {
    proptest::collection::vec((0u8..6, any::<u32>(), any::<u8>(), 0u16..2_000), 1..6)
}

fn apply(seed: &[u8], edits: &[Edit]) -> Vec<u8> {
    let mut data = seed.to_vec();
    for (kind, position, pick, length) in edits {
        if data.is_empty() {
            data.push(b'{');
        }
        let at = *position as usize % data.len();
        let token = TOKENS[usize::from(*pick) % TOKENS.len()];
        let length = usize::from(*length);
        match kind {
            0 => data[at] = token[0],
            1 => {
                data.remove(at);
            }
            2 => {
                data.splice(at..at, token.iter().copied());
            }
            3 => {
                let end = (at + length % 64 + 1).min(data.len());
                let slice = data[at..end].to_vec();
                data.splice(end..end, slice);
            }
            4 => data.truncate(at),
            _ => {
                let run = token.repeat(length % 600 + 1);
                data.splice(at..at, run);
            }
        }
        data.truncate(MAX_BYTES);
    }
    data
}

fn config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CASES),
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..Config::default()
    }
}

struct Root(PathBuf);

impl Root {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-fuzz-frames-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create root failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"frames\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write manifest failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), "pub fn one() -> u32 {\n    1\n}\n")
            .map_err(|error| format!("write source failed: {error}"))?;
        Ok(Self(root))
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn judge(status: std::process::ExitStatus) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return Err(format!("server died from signal {signal}"));
        }
    }
    #[cfg(windows)]
    {
        // Windows has no signals: a stack overflow or abort surfaces as an
        // NTSTATUS exit code (0xC00000FD stack overflow, 0xC0000409 fast-fail).
        if let Some(code) = status.code() {
            let code = u32::from_ne_bytes(code.to_ne_bytes());
            if code & 0xF000_0000 == 0xC000_0000 {
                return Err(format!("server crashed with status {code:#x}"));
            }
        }
    }
    if status.code() == Some(101) {
        return Err("server panicked (exit 101)".to_string());
    }
    Ok(())
}

/// Feeds `input` to the server, closes stdin, and waits up to the deadline.
fn drive(root: &Root, args: &[&str], input: Vec<u8>) -> Result<(), String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .current_dir(&root.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("spawn ripr failed: {error}"))?;
    let Some(mut stdin) = child.stdin.take() else {
        return Err("child exposed no stdin".to_string());
    };
    // Write from a thread: a server that stops reading must not block the test.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let started = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break judge(status),
            Ok(None) if started.elapsed() >= DEADLINE => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("no exit within {DEADLINE:?}"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => break Err(format!("wait failed: {error}")),
        }
    };
    let _ = writer.join();
    result
}

fn mcp_session() -> Vec<u8> {
    let messages = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fuzz","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ripr_status","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#,
    ];
    messages
        .iter()
        .flat_map(|m| format!("{m}\n").into_bytes())
        .collect()
}

fn frame(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fn lsp_session() -> Vec<u8> {
    let bodies = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/fuzz/src/lib.rs","languageId":"rust","version":1,"text":"pub fn one() -> u32 {\n    1\n}\n"}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///tmp/fuzz/src/lib.rs"},"position":{"line":1,"character":4}}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ];
    bodies.iter().flat_map(|body| frame(body)).collect()
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn mcp_frames_never_panic_or_hang(edits in edits()) {
        let root = Root::create().map_err(TestCaseError::fail)?;
        let input = apply(&mcp_session(), &edits);
        drive(&root, &["mcp", "--stdio", "--root", &root.0.to_string_lossy()], input.clone())
            .map_err(|error| TestCaseError::fail(format!("{error}\ninput bytes: {input:?}")))?;
    }

    #[test]
    fn lsp_frames_never_panic_or_hang(edits in edits()) {
        let root = Root::create().map_err(TestCaseError::fail)?;
        let input = apply(&lsp_session(), &edits);
        drive(&root, &["lsp", "--stdio"], input.clone())
            .map_err(|error| TestCaseError::fail(format!("{error}\ninput bytes: {input:?}")))?;
    }
}

/// Canary: the unmutated sessions must be answered, so the suites above reach
/// the protocol handlers instead of failing at spawn or argument parsing.
#[test]
fn unmutated_sessions_get_answers() -> Result<(), String> {
    let root = Root::create()?;
    for (args, session) in [
        (
            vec!["mcp", "--stdio", "--root", root.0.to_str().unwrap_or(".")],
            mcp_session(),
        ),
        (vec!["lsp", "--stdio"], lsp_session()),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(&args)
            .current_dir(&root.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("spawn failed: {error}"))?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&session);
        });
        let output = child
            .wait_with_output()
            .map_err(|error| format!("wait failed: {error}"))?;
        let _ = writer.join();
        let text = String::from_utf8_lossy(&output.stdout);
        if !text.contains("\"id\":1") && !text.contains("\"id\": 1") {
            return Err(format!("{args:?}: no initialize response: {text}"));
        }
    }
    Ok(())
}
