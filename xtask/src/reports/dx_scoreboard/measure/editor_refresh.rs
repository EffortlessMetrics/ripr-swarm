//! Editor-refresh instrument for `speed.editor_refresh_ms` and
//! `speed.editor_refresh_unanswered_progress_ms` (#5317, #9149) and the
//! server's peak memory, `speed.editor_refresh_peak_rss_mb`.
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
const TERMINAL_STATES: [&str; 5] = ["succeeded", "failed", "cancelled", "superseded", "stopped"];

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
    /// Peak resident set (`VmHWM`) of the language-server process once the
    /// edited save settled; `None` off Linux.
    pub(super) peak_rss_bytes: Option<u64>,
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
    match session.wait_attempt(deadline, None, Some(OPENING_GRACE)) {
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
    let state = match session.wait_attempt(deadline, Some("did_save"), None) {
        Wait::Terminal(state) => state,
        Wait::NoAttempt | Wait::Deadline => "no_terminal_status".to_string(),
        Wait::Closed => "lsp_closed".to_string(),
    };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    // Read while the server is still alive: the peak is only visible in
    // /proc until the process exits.
    let peak_rss_bytes = crate::run::proc_peak_rss_bytes(session.child.id());
    session.shutdown();
    Ok(Some(EditorRefresh {
        ms,
        state,
        file: relative.display().to_string(),
        peak_rss_bytes,
    }))
}

/// Outcome of the two resilience checks run against one changed Rust file.
#[derive(Debug, PartialEq)]
pub(super) struct EditorResilience {
    pub(super) file: String,
    /// Reason each failed check failed; empty when both held.
    pub(super) failures: Vec<String>,
}

/// Saves sent back to back in the burst check.
const BURST_SAVES: u32 = 4;

/// Gap between burst saves: shorter than any analysis, so each save lands
/// while the previous refresh is still in flight, as fast typing-and-saving does.
const BURST_GAP: Duration = Duration::from_millis(100);

/// How long the terminated server runs into its refresh before it is terminated.
const KILL_AFTER: Duration = Duration::from_millis(500);

/// Two failure modes a long editor session meets that a single clean save
/// never does. A burst of saves must settle on one `succeeded` refresh (the
/// stale ones `superseded`, none stuck), and a server terminated mid-refresh must
/// leave nothing behind that stops the next server from refreshing the same
/// checkout. `Ok(None)` when the change holds no Rust file to open.
pub(super) fn measure_editor_resilience(
    binary: &Path,
    checkout: &Path,
    base: &str,
    envs: &[(&str, &str)],
    timeout: Duration,
) -> Result<Option<EditorResilience>, String> {
    let Some(relative) = changed_rust_file(checkout, base)? else {
        return Ok(None);
    };
    let path = checkout.join(&relative);
    let original = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let _restore = RestoreFile {
        path: path.clone(),
        original: original.clone(),
    };
    let uri = file_uri(&path);
    let deadline = Instant::now() + timeout;
    let mut failures = Vec::new();

    let mut session = Session::start(binary, checkout, envs, base, ProgressReply::Answer)?;
    session.initialize(checkout, deadline)?;
    open_and_warm(&mut session, &uri, &original, deadline)?;

    // Burst: several edited saves, each before the last refresh can finish.
    let mut version = 2;
    for index in 0..BURST_SAVES {
        let text = format!("{original}\n// ripr dx-scoreboard burst edit {index}\n");
        save_edit(&mut session, &path, &uri, &text, version)?;
        version += 1;
        std::thread::sleep(BURST_GAP);
    }
    match session.wait_attempt(deadline, Some("did_save"), None) {
        Wait::Terminal(state) if state == "succeeded" => {}
        Wait::Terminal(state) => {
            failures.push(format!("burst of {BURST_SAVES} saves ended `{state}`"));
        }
        Wait::NoAttempt | Wait::Deadline => {
            failures.push(format!(
                "burst of {BURST_SAVES} saves never reached a terminal status"
            ));
        }
        Wait::Closed => failures.push("server closed during the save burst".to_string()),
    }

    // Crash: kill the server while a refresh is running, then refresh again
    // from a fresh process on the same checkout.
    let text = format!("{original}\n// ripr dx-scoreboard kill edit\n");
    if save_edit(&mut session, &path, &uri, &text, version).is_ok() {
        std::thread::sleep(KILL_AFTER);
    }
    let _ = session.child.kill();
    drop(session);
    let mut next = Session::start(binary, checkout, envs, base, ProgressReply::Answer)?;
    next.initialize(checkout, deadline)?;
    open_and_warm(&mut next, &uri, &original, deadline)?;
    save_edit(
        &mut next,
        &path,
        &uri,
        &format!("{original}{EDIT_MARKER}"),
        2,
    )?;
    match next.wait_attempt(deadline, Some("did_save"), None) {
        Wait::Terminal(state) if state == "succeeded" => {}
        Wait::Terminal(state) => {
            failures.push(format!("refresh after a terminated server ended `{state}`"));
        }
        Wait::NoAttempt | Wait::Deadline => {
            failures
                .push("refresh after a terminated server never reached a terminal status".into());
        }
        Wait::Closed => failures.push("restarted server closed during its refresh".to_string()),
    }
    next.shutdown();
    Ok(Some(EditorResilience {
        file: relative.display().to_string(),
        failures,
    }))
}

/// Open the document and wait out the opening refresh, as the timed check does.
fn open_and_warm(
    session: &mut Session,
    uri: &str,
    original: &str,
    deadline: Instant,
) -> Result<(), String> {
    session.notify(
        "textDocument/didOpen",
        json!({"textDocument": {
            "uri": uri, "languageId": "rust", "version": 1, "text": original,
        }}),
    )?;
    match session.wait_attempt(deadline, None, Some(OPENING_GRACE)) {
        Wait::Terminal(state) if state != "succeeded" => {
            Err(format!("opening refresh ended `{state}`"))
        }
        Wait::Terminal(_) | Wait::NoAttempt => Ok(()),
        Wait::Deadline => Err("opening refresh did not finish before the deadline".into()),
        Wait::Closed => Err("lsp child closed its output during the opening refresh".into()),
    }
}

fn save_edit(
    session: &mut Session,
    path: &Path,
    uri: &str,
    text: &str,
    version: u32,
) -> Result<(), String> {
    fs::write(path, text).map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    session.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": uri, "version": version},
            "contentChanges": [{"text": text}],
        }),
    )?;
    session.notify(
        "textDocument/didSave",
        json!({"textDocument": {"uri": uri}}),
    )
}

/// First Rust file the base..HEAD change touches that still exists.
fn changed_rust_file(checkout: &Path, base: &str) -> Result<Option<PathBuf>, String> {
    let names = super::git(
        Some(checkout),
        &[
            "diff",
            "--name-only",
            "-z",
            "--diff-filter=AM",
            base,
            "HEAD",
        ],
    )?;
    // NUL-separated, so Git does not quote a name with unusual bytes.
    Ok(names
        .split('\0')
        .filter(|name| name.ends_with(".rs"))
        .map(PathBuf::from)
        .find(|relative| checkout.join(relative).is_file()))
}

/// `file://` URI with every byte outside the unreserved set (and `/`)
/// percent-encoded, so a checkout path holding a space or `#` stays one URI.
/// A drive-absolute Windows path becomes `file:///C:/dir`, the form the
/// server's URI parser reads.
fn file_uri(path: &Path) -> String {
    let native = path.display().to_string().replace('\\', "/");
    // `canonicalize` on Windows prefixes the drive path with `\\?\`; the server reads it without.
    let text = native.strip_prefix("//?/").unwrap_or(&native);
    let drive = text.as_bytes();
    let drive_absolute = drive.len() >= 2 && drive[0].is_ascii_alphabetic() && drive[1] == b':';
    let mut uri = String::from("file://");
    if drive_absolute {
        uri.push('/');
    }
    for (index, byte) in text.bytes().enumerate() {
        let drive_colon = drive_absolute && index == 1;
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'/' | b'-' | b'.' | b'_' | b'~')
            || drive_colon
        {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// Follows one refresh through its statuses. Only an attempt started by this
/// phase's own event counts (`reason`; `None` takes any), so an earlier request
/// still in flight cannot stand in for it, and only a terminal status naming
/// the adopted attempt ends the wait. An attempt that ends `superseded` hands
/// over to the attempt that started after it, whatever started that one.
struct AttemptTracker<'a> {
    reason: Option<&'a str>,
    current: Option<String>,
    handed_over: bool,
    seen_status: bool,
    /// Every attempt id seen so far: a handover takes only an attempt first
    /// seen after the adopted one, so a stale status re-published for an
    /// older attempt cannot stand in.
    seen_attempts: Vec<String>,
    /// Attempts that started while another was adopted, with their terminal
    /// state if it already arrived: the replacement can start before the
    /// superseded attempt reports.
    successors: Vec<(String, Option<String>)>,
}

impl<'a> AttemptTracker<'a> {
    fn new(reason: Option<&'a str>) -> Self {
        Self {
            reason,
            current: None,
            handed_over: false,
            seen_status: false,
            seen_attempts: Vec::new(),
            successors: Vec::new(),
        }
    }

    fn seen_status(&self) -> bool {
        self.seen_status
    }

    /// The terminal state once the adopted attempt ends.
    fn observe(&mut self, status: AnalysisStatus) -> Option<String> {
        let AnalysisStatus {
            state,
            attempt,
            reason,
        } = status;
        if !TERMINAL_STATES.contains(&state.as_str()) {
            // A started attempt ends the start grace; a terminal status
            // re-published for an attempt never seen starting does not.
            self.seen_status = true;
            let is_new = attempt
                .as_ref()
                .is_some_and(|id| !self.seen_attempts.contains(id));
            let adopt = if self.handed_over {
                is_new && self.current.is_none()
            } else {
                self.reason.is_none() || reason.as_deref() == self.reason
            };
            if let (true, Some(id)) = (is_new, &attempt) {
                self.seen_attempts.push(id.clone());
                if !adopt && self.current.is_some() {
                    self.successors.push((id.clone(), None));
                }
            }
            if adopt {
                self.current = attempt;
            }
            return None;
        }
        let id = attempt?;
        if Some(&id) != self.current.as_ref() {
            if let Some(entry) = self.successors.iter_mut().find(|(seen, _)| *seen == id) {
                entry.1 = Some(state);
            }
            return None;
        }
        if state != "superseded" {
            return Some(state);
        }
        self.current = None;
        self.handed_over = true;
        let mut pending = std::mem::take(&mut self.successors).into_iter();
        while let Some((successor, ended)) = pending.next() {
            match ended {
                Some(state) if state == "superseded" => {}
                Some(state) => return Some(state),
                None => {
                    self.current = Some(successor);
                    // Later successors wait for this one to end in turn.
                    self.successors = pending.collect();
                    break;
                }
            }
        }
        None
    }
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
    fn wait_attempt(
        &mut self,
        deadline: Instant,
        reason: Option<&str>,
        start_grace: Option<Duration>,
    ) -> Wait {
        let start_by = start_grace.map(|grace| Instant::now() + grace);
        let mut tracker = AttemptTracker::new(reason);
        loop {
            // The grace covers only the silence before the first status of
            // any kind; once the server is busy the attempt may queue behind it.
            let limit = match start_by {
                Some(start_by) if !tracker.seen_status() => start_by.min(deadline),
                _ => deadline,
            };
            let message = match self.next_message_until(limit) {
                Ok(Some(message)) => message,
                Ok(None) if !tracker.seen_status() && start_by.is_some_and(|by| by <= deadline) => {
                    return Wait::NoAttempt;
                }
                Ok(None) => return Wait::Deadline,
                Err(()) => return Wait::Closed,
            };
            let Some(status) = analysis_status(&message) else {
                continue;
            };
            if let Some(state) = tracker.observe(status) {
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

/// State, attempt id and reason of a `ripr/analysisStatus` notification.
struct AnalysisStatus {
    state: String,
    attempt: Option<String>,
    reason: Option<String>,
}

fn analysis_status(message: &Value) -> Option<AnalysisStatus> {
    if message.get("method").and_then(Value::as_str) != Some("ripr/analysisStatus") {
        return None;
    }
    let params = message.get("params")?;
    let state = params.get("state")?.as_str()?.to_string();
    let text = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_string);
    Some(AnalysisStatus {
        state,
        attempt: text("attempt_id"),
        reason: text("reason"),
    })
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

    /// Real-server check of the resilience instrument. Needs a built binary,
    /// so it runs only when `RIPR_DX_EDITOR_BINARY` names one (CI builds none
    /// for xtask tests); the instrument's logic is otherwise unexercised.
    #[test]
    fn resilience_holds_for_a_small_workspace_with_a_real_server() -> Result<(), String> {
        let Some(binary) = std::env::var_os("RIPR_DX_EDITOR_BINARY") else {
            return Ok(());
        };
        let dir = std::env::temp_dir().join(format!("ripr-resilience-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).map_err(|err| err.to_string())?;
        let write = |body: &str| fs::write(dir.join("src/lib.rs"), body).map_err(|e| e.to_string());
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"resfix\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .map_err(|err| err.to_string())?;
        write("pub fn f(x: u32) -> u32 { x }\n")?;
        let git = |args: &[&str]| super::super::git(Some(&dir), args).map(|_| ());
        git(&["init", "-q", "-b", "main"])?;
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "add", "-A"])?;
        git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "base",
        ])?;
        write(
            "pub fn f(x: u32) -> u32 { if x > 1 { x + 1 } else { x } }\n#[cfg(test)]\nmod t { #[test] fn a() { assert_eq!(super::f(1), 1); } }\n",
        )?;
        git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qam",
            "change",
        ])?;
        let result = measure_editor_resilience(
            Path::new(&binary),
            &dir,
            "HEAD~1",
            &[],
            Duration::from_mins(2),
        );
        let _ = fs::remove_dir_all(&dir);
        let resilience = result?.ok_or("fixture changed no Rust file")?;
        assert!(resilience.failures.is_empty(), "{:?}", resilience.failures);
        Ok(())
    }
    use super::*;
    use std::io::Cursor;

    fn framed(body: &str) -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
    }

    fn status(state: &str, attempt: &str, reason: &str) -> AnalysisStatus {
        AnalysisStatus {
            state: state.to_string(),
            attempt: Some(attempt.to_string()),
            reason: Some(reason.to_string()),
        }
    }

    #[test]
    fn file_uri_drops_the_windows_verbatim_prefix() {
        let verbatim = ["\\\\?", "C:", "work", "x"].join("\\");
        assert_eq!(file_uri(Path::new(&verbatim)), "file:///C:/work/x");
    }

    #[test]
    fn a_stale_attempt_of_another_reason_does_not_stand_in_for_the_save() {
        let mut tracker = AttemptTracker::new(Some("did_save"));
        assert_eq!(tracker.observe(status("running", "1", "did_open")), None);
        assert_eq!(tracker.observe(status("succeeded", "1", "did_open")), None);
        assert_eq!(tracker.observe(status("running", "2", "did_save")), None);
        assert_eq!(
            tracker.observe(status("succeeded", "2", "did_save")),
            Some("succeeded".to_string())
        );
    }

    #[test]
    fn a_superseded_save_hands_over_to_the_next_attempt_of_any_reason() {
        let mut tracker = AttemptTracker::new(Some("did_save"));
        assert_eq!(tracker.observe(status("running", "2", "did_save")), None);
        assert_eq!(tracker.observe(status("superseded", "2", "did_save")), None);
        assert_eq!(
            tracker.observe(status("running", "3", "config_reload")),
            None
        );
        assert_eq!(
            tracker.observe(status("succeeded", "3", "config_reload")),
            Some("succeeded".to_string())
        );
    }

    #[test]
    fn a_stopped_attempt_ends_the_wait_but_an_earlier_stopped_one_does_not() {
        let mut tracker = AttemptTracker::new(Some("did_save"));
        assert_eq!(tracker.observe(status("stopped", "1", "did_open")), None);
        assert_eq!(tracker.observe(status("queued", "2", "did_save")), None);
        assert_eq!(
            tracker.observe(status("stopped", "2", "did_save")),
            Some("stopped".to_string())
        );
    }

    #[test]
    fn a_stale_older_attempt_after_handover_is_not_adopted() {
        let mut tracker = AttemptTracker::new(Some("did_save"));
        assert_eq!(tracker.observe(status("running", "1", "did_open")), None);
        assert_eq!(tracker.observe(status("running", "2", "did_save")), None);
        assert_eq!(tracker.observe(status("superseded", "2", "did_save")), None);
        assert_eq!(tracker.observe(status("running", "1", "did_open")), None);
        assert_eq!(tracker.observe(status("succeeded", "1", "did_open")), None);
        assert_eq!(
            tracker.observe(status("running", "3", "watched_input")),
            None
        );
        assert_eq!(
            tracker.observe(status("succeeded", "3", "watched_input")),
            Some("succeeded".to_string())
        );
    }

    #[test]
    fn a_republished_terminal_does_not_end_the_start_grace() {
        let mut tracker = AttemptTracker::new(None);
        assert_eq!(tracker.observe(status("succeeded", "1", "did_open")), None);
        assert!(!tracker.seen_status());
    }

    #[test]
    fn a_replacement_started_before_the_save_reports_superseded_is_adopted() {
        let mut tracker = AttemptTracker::new(Some("did_save"));
        assert_eq!(tracker.observe(status("running", "2", "did_save")), None);
        assert_eq!(
            tracker.observe(status("running", "3", "config_reload")),
            None
        );
        assert_eq!(tracker.observe(status("superseded", "2", "did_save")), None);
        assert_eq!(
            tracker.observe(status("succeeded", "3", "config_reload")),
            Some("succeeded".to_string())
        );
    }

    #[test]
    fn a_replacement_that_ended_before_the_supersession_report_counts() {
        let mut tracker = AttemptTracker::new(Some("did_save"));
        assert_eq!(tracker.observe(status("running", "2", "did_save")), None);
        assert_eq!(
            tracker.observe(status("running", "3", "config_reload")),
            None
        );
        assert_eq!(
            tracker.observe(status("succeeded", "3", "config_reload")),
            None
        );
        assert_eq!(
            tracker.observe(status("superseded", "2", "did_save")),
            Some("succeeded".to_string())
        );
    }

    #[test]
    fn a_second_replacement_survives_the_first_handover() {
        let mut tracker = AttemptTracker::new(Some("did_save"));
        assert_eq!(tracker.observe(status("running", "2", "did_save")), None);
        assert_eq!(
            tracker.observe(status("running", "3", "config_reload")),
            None
        );
        assert_eq!(
            tracker.observe(status("running", "4", "watched_input")),
            None
        );
        assert_eq!(tracker.observe(status("superseded", "2", "did_save")), None);
        assert_eq!(
            tracker.observe(status("superseded", "3", "config_reload")),
            None
        );
        assert_eq!(
            tracker.observe(status("succeeded", "4", "watched_input")),
            Some("succeeded".to_string())
        );
    }

    #[test]
    fn the_opening_wait_takes_an_attempt_of_any_reason() {
        let mut tracker = AttemptTracker::new(None);
        assert!(!tracker.seen_status());
        assert_eq!(
            tracker.observe(status("running", "1", "config_reload")),
            None
        );
        assert!(tracker.seen_status());
        assert_eq!(
            tracker.observe(status("succeeded", "1", "config_reload")),
            Some("succeeded".to_string())
        );
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
        let parsed = analysis_status(&status).map(|s| (s.state, s.attempt, s.reason));
        assert_eq!(
            parsed,
            Some(("running".to_string(), Some("4".to_string()), None))
        );
        let other = json!({"method": "$/progress", "params": {"state": "succeeded"}});
        assert!(analysis_status(&other).is_none());
    }

    #[test]
    fn file_uri_encodes_bytes_outside_the_unreserved_set() {
        assert_eq!(
            file_uri(Path::new("/tmp/a b#c/x_y-z.rs")),
            "file:///tmp/a%20b%23c/x_y-z.rs"
        );
        assert_eq!(
            file_uri(Path::new(&["C:", "work", "src", "lib.rs"].join("\\"))),
            "file:///C:/work/src/lib.rs"
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
