//! Editor-refresh instrument for `speed.editor_refresh_ms` and
//! `speed.editor_refresh_unanswered_progress_ms` (#5317, #9149).
//!
//! Drives `ripr lsp --stdio` the way an editor does: initialize with
//! work-done-progress and pull configuration advertised, open one changed
//! Rust file, wait for the opening refresh to finish (this warms the saved
//! snapshot), then save an edited copy and time the second refresh to its
//! terminal `ripr/analysisStatus`. The variant that never answers
//! `window/workDoneProgress/create` measures the same refresh with the
//! client silent about progress, so a refresh that waits on that request
//! shows up as the 30 s client-request timeout instead of passing as a
//! slow analysis.

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Marker appended to the opened file so the second save changes the bytes.
/// An unchanged save is skipped by the server, which would time nothing.
const EDIT_MARKER: &str = "\n// ripr dx-scoreboard editor-refresh edit\n";

/// Terminal `ripr/analysisStatus` states; every other state is in flight.
const TERMINAL_STATES: [&str; 4] = ["succeeded", "failed", "cancelled", "superseded"];

/// How long to wait for the opening refresh to start before concluding the
/// server already held analysis state and scheduled none.
const OPENING_GRACE: Duration = Duration::from_secs(10);

enum Wait {
    Terminal(String),
    /// No attempt started within the grace period.
    NoAttempt,
    Deadline,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProgressReply {
    /// Answer `window/workDoneProgress/create` like an ordinary editor.
    Answer,
    /// Never answer it, as a client that advertises progress but is stuck.
    Withhold,
}

#[derive(Debug, PartialEq)]
pub(super) struct EditorRefresh {
    /// Milliseconds from the edited save to the terminal analysis status.
    pub(super) ms: f64,
    /// Terminal `state` the server reported for that refresh.
    pub(super) state: String,
    pub(super) file: String,
}

impl EditorRefresh {
    pub(super) fn complete(&self) -> bool {
        self.state == "succeeded"
    }
}

/// `Ok(None)` when the change holds no Rust file to open: a property of the
/// corpus pin, not of ripr, so the caller reports it as not measured.
pub(super) fn measure_editor_refresh(
    binary: &Path,
    checkout: &Path,
    base: &str,
    envs: &[(&str, &str)],
    reply: ProgressReply,
    timeout: Duration,
) -> Result<Option<EditorRefresh>, String> {
    let Some(relative) = changed_rust_file(checkout, base)? else {
        return Ok(None);
    };
    let path = checkout.join(&relative);
    let original = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    // One budget for the whole session, so slow phases cannot multiply it.
    let deadline = Instant::now() + timeout;
    let mut session = Session::start(binary, checkout, envs, base, reply)?;
    // Restore the checkout even when the session fails half way.
    let _restore = RestoreFile {
        path: path.clone(),
        original: original.clone(),
    };
    session.initialize(checkout, deadline)?;
    let uri = file_uri(&path);
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument": {
            "uri": uri, "languageId": "rust", "version": 1, "text": original,
        }}),
    )?;
    // A didOpen the server already holds state for schedules no refresh, so
    // only wait for the opening refresh when one starts.
    match session.wait_attempt(deadline, Some(OPENING_GRACE)) {
        Wait::Terminal(state) if state != "succeeded" => {
            return Err(format!("opening refresh ended `{state}`"));
        }
        Wait::Terminal(_) | Wait::NoAttempt => {}
        Wait::Deadline => return Err("opening refresh did not finish before the deadline".into()),
        Wait::Closed => return Err("lsp child closed its output during the opening refresh".into()),
    }

    let edited = format!("{original}{EDIT_MARKER}");
    fs::write(&path, &edited)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    session.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": edited}],
        }),
    )?;
    let started = Instant::now();
    session.notify(
        "textDocument/didSave",
        json!({"textDocument": {"uri": uri}}),
    )?;
    // From here a stall or a crash is the product's behavior, not the
    // instrument's: report the elapsed time with a non-succeeded state.
    let state = match session.wait_attempt(deadline, None) {
        Wait::Terminal(state) => state,
        Wait::NoAttempt | Wait::Deadline => "no_terminal_status".to_string(),
        Wait::Closed => "lsp_closed".to_string(),
    };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    session.shutdown();
    Ok(Some(EditorRefresh {
        ms,
        state,
        file: relative.display().to_string(),
    }))
}

/// First Rust file the base..HEAD change touches that still exists.
fn changed_rust_file(checkout: &Path, base: &str) -> Result<Option<PathBuf>, String> {
    let names = super::git(
        Some(checkout),
        &["diff", "--name-only", "--diff-filter=AM", base, "HEAD"],
    )?;
    Ok(names
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(".rs"))
        .map(PathBuf::from)
        .find(|relative| checkout.join(relative).is_file()))
}

/// `file://` URI with every byte outside the unreserved set (and `/`)
/// percent-encoded, so a checkout path holding a space or `#` stays one URI.
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.display().to_string().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'.' | b'_' | b'~') {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

struct RestoreFile {
    path: PathBuf,
    original: String,
}

impl Drop for RestoreFile {
    fn drop(&mut self) {
        // Best effort: the scoreboard checkout is scratch, but a broken
        // restore would leave an edited file in the next measurement.
        let _ = fs::write(&self.path, &self.original);
    }
}

struct Session {
    child: Child,
    stdin: ChildStdin,
    inbox: Receiver<Value>,
    next_id: u64,
    base: String,
    reply: ProgressReply,
}

impl Session {
    fn start(
        binary: &Path,
        checkout: &Path,
        envs: &[(&str, &str)],
        base: &str,
        reply: ProgressReply,
    ) -> Result<Self, String> {
        let mut command = Command::new(binary);
        command
            .args(["lsp", "--stdio"])
            .current_dir(checkout)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (key, value) in envs {
            command.env(key, value);
        }
        let mut child = command
            .spawn()
            .map_err(|err| format!("failed to start `{} lsp --stdio`: {err}", binary.display()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "lsp child has no stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "lsp child has no stdout".to_string())?;
        let (sender, inbox) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(message)) = read_frame(&mut reader) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            inbox,
            next_id: 0,
            base: base.to_string(),
            reply,
        })
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len())
            .and_then(|()| self.stdin.flush())
            .map_err(|err| format!("failed to write to the lsp child: {err}"))
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn initialize(&mut self, checkout: &Path, deadline: Instant) -> Result<(), String> {
        self.next_id += 1;
        let id = self.next_id;
        let root = file_uri(checkout);
        self.send(&json!({
            "jsonrpc": "2.0", "id": id, "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root,
                "workspaceFolders": [{"uri": root, "name": "ws"}],
                "initializationOptions": {"baseRef": self.base},
                "capabilities": {
                    "textDocument": {"synchronization": {"didSave": true}},
                    "window": {"workDoneProgress": true},
                    "workspace": {"configuration": true, "workspaceFolders": true},
                },
            },
        }))?;
        loop {
            let message = self.next_message(deadline, "initialize")?;
            if message.get("id") == Some(&json!(id)) && message.get("method").is_none() {
                break;
            }
        }
        self.notify("initialized", json!({}))
    }

    /// Wait for one refresh attempt to reach a terminal status. Only a
    /// status that names the attempt this wait saw start counts, so a
    /// terminal status re-published for an earlier attempt cannot end it.
    /// An attempt that ends `superseded` hands over to the next one.
    /// `start_grace` bounds how long to wait for any attempt to start.
    fn wait_attempt(&mut self, deadline: Instant, start_grace: Option<Duration>) -> Wait {
        let start_by = start_grace.map(|grace| Instant::now() + grace);
        let mut current: Option<String> = None;
        loop {
            let limit = match (&current, start_by) {
                (None, Some(start_by)) => start_by.min(deadline),
                _ => deadline,
            };
            let message = match self.next_message_until(limit) {
                Ok(Some(message)) => message,
                Ok(None) if current.is_none() && start_by.is_some_and(|by| by <= deadline) => {
                    return Wait::NoAttempt;
                }
                Ok(None) => return Wait::Deadline,
                Err(()) => return Wait::Closed,
            };
            let Some((state, attempt)) = analysis_status(&message) else {
                continue;
            };
            if !TERMINAL_STATES.contains(&state.as_str()) {
                current = attempt;
                continue;
            }
            if attempt.is_some() && attempt == current {
                if state == "superseded" {
                    current = None;
                    continue;
                }
                return Wait::Terminal(state);
            }
        }
    }

    /// Next message before `limit`, answering server-to-client requests on
    /// the way. `Ok(None)` at the limit, `Err` when the child closed.
    fn next_message_until(&mut self, limit: Instant) -> Result<Option<Value>, ()> {
        loop {
            let left = limit.saturating_duration_since(Instant::now());
            let message = match self.inbox.recv_timeout(left) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => return Err(()),
            };
            if let Some(response) = server_request_reply(&message, &self.base, self.reply) {
                if self.send(&response).is_err() {
                    return Err(());
                }
                continue;
            }
            return Ok(Some(message));
        }
    }

    /// Next message before the session deadline; used where a timeout is an
    /// instrument failure.
    fn next_message(&mut self, deadline: Instant, what: &str) -> Result<Value, String> {
        match self.next_message_until(deadline) {
            Ok(Some(message)) => Ok(message),
            Ok(None) => Err(format!("no reply during {what} before the deadline")),
            Err(()) => Err(format!("lsp child closed its output during {what}")),
        }
    }

    fn shutdown(&mut self) {
        self.next_id += 1;
        let id = self.next_id;
        let _ = self.send(&json!({
            "jsonrpc": "2.0", "id": id, "method": "shutdown", "params": null,
        }));
        let _ = self.notify("exit", Value::Null);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Reap the child so a failed or timed-out session leaves no process.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `(state, attempt_id)` of a `ripr/analysisStatus` notification.
fn analysis_status(message: &Value) -> Option<(String, Option<String>)> {
    if message.get("method").and_then(Value::as_str) != Some("ripr/analysisStatus") {
        return None;
    }
    let params = message.get("params")?;
    let state = params.get("state")?.as_str()?.to_string();
    let attempt = params
        .get("attempt_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some((state, attempt))
}

/// The reply a client owes a server-to-client request, or `None` when the
/// message is not one (or the reply is deliberately withheld).
fn server_request_reply(message: &Value, base: &str, reply: ProgressReply) -> Option<Value> {
    let id = message.get("id")?;
    let method = message.get("method")?.as_str()?;
    let result = match method {
        "window/workDoneProgress/create" if reply == ProgressReply::Withhold => return None,
        "workspace/configuration" => {
            let items = message
                .get("params")
                .and_then(|params| params.get("items"))
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
            Value::Array(vec![json!({"baseRef": base}); items])
        }
        _ => Value::Null,
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// One Content-Length framed JSON-RPC message; `Ok(None)` at end of stream.
fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|err| format!("failed to read an lsp header: {err}"))?;
        if read == 0 {
            return Ok(None);
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().ok();
        }
    }
    let length = length.ok_or_else(|| "lsp frame without a Content-Length".to_string())?;
    let mut body = vec![0_u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|err| format!("failed to read an lsp body: {err}"))?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|err| format!("lsp frame is not JSON: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn framed(body: &str) -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
    }

    #[test]
    fn frames_are_read_in_order_until_end_of_stream() {
        let mut bytes = framed(r#"{"a":1}"#);
        bytes.extend(framed(r#"{"b":2}"#));
        let mut reader = Cursor::new(bytes);
        assert_eq!(read_frame(&mut reader), Ok(Some(json!({"a": 1}))));
        assert_eq!(read_frame(&mut reader), Ok(Some(json!({"b": 2}))));
        assert_eq!(read_frame(&mut reader), Ok(None));
    }

    #[test]
    fn a_frame_without_a_length_is_refused() {
        let mut reader = Cursor::new(b"Content-Type: x\r\n\r\n{}".to_vec());
        assert_eq!(
            read_frame(&mut reader),
            Err("lsp frame without a Content-Length".to_string())
        );
    }

    #[test]
    fn analysis_status_reads_state_and_attempt() {
        let status = json!({"method": "ripr/analysisStatus",
            "params": {"state": "running", "attempt_id": "4"}});
        assert_eq!(
            analysis_status(&status),
            Some(("running".to_string(), Some("4".to_string())))
        );
        let other = json!({"method": "$/progress", "params": {"state": "succeeded"}});
        assert_eq!(analysis_status(&other), None);
    }

    #[test]
    fn file_uri_encodes_bytes_outside_the_unreserved_set() {
        assert_eq!(
            file_uri(Path::new("/tmp/a b#c/x_y-z.rs")),
            "file:///tmp/a%20b%23c/x_y-z.rs"
        );
    }

    #[test]
    fn progress_create_is_answered_or_withheld_by_mode() {
        let create = json!({"id": 7, "method": "window/workDoneProgress/create",
            "params": {"token": "t"}});
        let answered = server_request_reply(&create, "HEAD~1", ProgressReply::Answer);
        assert_eq!(
            answered,
            Some(json!({"jsonrpc": "2.0", "id": 7, "result": null}))
        );
        assert_eq!(
            server_request_reply(&create, "HEAD~1", ProgressReply::Withhold),
            None
        );
    }

    #[test]
    fn configuration_pull_gets_one_base_ref_per_item() {
        let pull = json!({"id": 3, "method": "workspace/configuration",
            "params": {"items": [{"section": "ripr"}, {"section": "ripr.x"}]}});
        let reply = server_request_reply(&pull, "abc", ProgressReply::Withhold);
        assert_eq!(
            reply,
            Some(json!({"jsonrpc": "2.0", "id": 3,
                "result": [{"baseRef": "abc"}, {"baseRef": "abc"}]}))
        );
    }

    #[test]
    fn notifications_are_not_replied_to() {
        let notification = json!({"method": "window/logMessage", "params": {}});
        assert_eq!(
            server_request_reply(&notification, "b", ProgressReply::Answer),
            None
        );
    }
}
