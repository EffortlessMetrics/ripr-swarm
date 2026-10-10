//! Seam-inventory degradation forced through a production refresh (#7147, seam half).
//!
//! RIPR-SPEC-0141 promises that a seam-inventory failure degrades the refresh
//! on the wire: a `seam_inventory`/`failed`/`seam_inventory_failed` component
//! outcome, a `limited` run, exactly one WARNING naming the component and its
//! recovery, and ordinary findings still published. The gap-ledger half of
//! #7147 landed as an in-process framed-duplex test (#7170); the seam half
//! cannot plant its trigger in-process — Rust 2024 made `std::env::set_var`
//! `unsafe`, and this workspace forbids `unsafe` — so this test drives the
//! real `ripr lsp --stdio` binary as a child process instead. `Command::env`
//! on the child is safe and needs no `unsafe`.
//!
//! Forcing chain (all production code, no test hooks):
//! `RIPR_REPO_EXPOSURE_SEAM_LIMIT=not-a-count` makes
//! `repo_exposure_seam_limit()` return `Err` (the #4529 fail-closed parser),
//! `inventory_classified_seams_report_at_with_config` propagates it with `?`
//! before the cache lookup and the index build, and the LSP assembly site
//! converts the `Err` into the `seam_inventory_failed` component outcome.
//!
//! Two sequential spawns share one git fixture: a control without the
//! override (proves the fixture would refresh `full` with a `complete` seam
//! inventory, and captures the finding set the degraded run must keep) and
//! the forced run (asserts the SPEC-0141 wire outcomes). The control removes
//! the variable from the child env, so ambient pollution can fake neither a
//! pass nor a fail. Every spawned server is terminated and reaped by
//! `SpawnSession::drop`, so a failing test cannot orphan a process that
//! would hold a file lock on the binary and break later builds.

use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

/// Per-read budget so a hung server fails fast instead of blocking CI.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
/// Budget for `ripr.refresh`, which runs workspace analysis.
const ANALYSIS_TIMEOUT: Duration = Duration::from_secs(90);
/// Budget for the process to terminate after `exit`.
const EXIT_TIMEOUT: Duration = Duration::from_secs(15);

/// The production seam-limit override
/// (`analysis/seam_inventory.rs::REPO_EXPOSURE_SEAM_LIMIT_ENV`).
const SEAM_LIMIT_ENV: &str = "RIPR_REPO_EXPOSURE_SEAM_LIMIT";
/// Not a seam count, not the `0` opt-out: the fail-closed parser rejects it.
const INVALID_SEAM_LIMIT: &str = "not-a-count";

/// The verbatim `ExposureClass` labels the server emits as finding
/// diagnostic codes (`domain/classification.rs` via
/// `lsp/diagnostic_catalog.rs::finding_code`). Seam and gap diagnostics
/// carry `ripr-`-prefixed codes, so membership in this set pins a diff
/// finding without reaching into crate internals.
const FINDING_CODES: [&str; 7] = [
    "exposed",
    "weakly_exposed",
    "reachable_unrevealed",
    "no_static_path",
    "infection_unknown",
    "propagation_unknown",
    "static_unknown",
];

/// The seeded boundary edit lands on `src/lib.rs` line 2 (1-based), so its
/// diff finding diagnostic starts at 0-based LSP line 1 (mirrors #7170).
const SEEDED_LINE: u64 = 1;

/// Synthetic URIs for the oracle-integrity controls: the fixture URI and a
/// different workspace's same-named lib that a suffix match would confuse.
const CONTROL_URI: &str = "file:///fixture/src/lib.rs";
const FOREIGN_URI: &str = "file:///elsewhere/src/lib.rs";

/// Events forwarded by the stdout reader thread.
enum WireEvent {
    Message(serde_json::Value),
    Failed(String),
}

/// Incremental `Content-Length` frame decoder for the server's stdout.
struct FrameReader<R> {
    inner: R,
    buffer: Vec<u8>,
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_content_length(headers: &str) -> Result<usize, String> {
    const NAME: &str = "Content-Length:";
    for line in headers.split("\r\n") {
        if let Some(value) = line
            .get(..NAME.len())
            .filter(|name| name.eq_ignore_ascii_case(NAME))
            .and_then(|_| line.get(NAME.len()..))
        {
            return value
                .trim()
                .parse::<usize>()
                .map_err(|err| format!("invalid Content-Length in {headers:?}: {err}"));
        }
    }
    Err(format!("missing Content-Length header in {headers:?}"))
}

impl<R: Read> FrameReader<R> {
    fn read_message(&mut self) -> Result<Option<serde_json::Value>, String> {
        loop {
            if let Some(header_end) = find_subslice(&self.buffer, b"\r\n\r\n") {
                let headers = std::str::from_utf8(&self.buffer[..header_end])
                    .map_err(|err| format!("frame headers are not UTF-8: {err}"))?;
                let content_length = parse_content_length(headers)?;
                let frame_len = header_end + 4 + content_length;
                if self.buffer.len() >= frame_len {
                    let body = self.buffer[header_end + 4..frame_len].to_vec();
                    self.buffer.drain(..frame_len);
                    let text = String::from_utf8(body)
                        .map_err(|err| format!("frame body is not UTF-8: {err}"))?;
                    let value = serde_json::from_str(&text)
                        .map_err(|err| format!("frame body is not JSON: {err}; body: {text}"))?;
                    return Ok(Some(value));
                }
            }
            let mut chunk = [0_u8; 4096];
            let read = self
                .inner
                .read(&mut chunk)
                .map_err(|err| format!("reading server stdout: {err}"))?;
            if read == 0 {
                if self.buffer.is_empty() {
                    return Ok(None);
                }
                return Err(format!(
                    "server stdout hit EOF mid-frame with {} buffered byte(s)",
                    self.buffer.len()
                ));
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

/// A live `ripr lsp --stdio` process plus its framed stdout stream.
struct SpawnSession {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<WireEvent>,
    next_id: u64,
}

impl SpawnSession {
    /// Spawn the real server. `Some(value)` plants the seam-limit override
    /// in the CHILD env only; `None` spawns the control with the variable
    /// removed, so an ambient export can fake neither a pass nor a fail.
    fn spawn(seam_limit_override: Option<&str>) -> Result<Self, String> {
        let binary = env!("CARGO_BIN_EXE_ripr");
        let mut command = Command::new(binary);
        command.args(["lsp", "--stdio"]);
        match seam_limit_override {
            Some(value) => {
                command.env(SEAM_LIMIT_ENV, value);
            }
            None => {
                command.env_remove(SEAM_LIMIT_ENV);
            }
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("failed to spawn `{binary} lsp --stdio`: {err}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "spawned server is missing a stdin pipe".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "spawned server is missing a stdout pipe".to_string())?;
        let (sender, events) = sync_channel(64);
        std::thread::spawn(move || {
            let mut reader = FrameReader {
                inner: stdout,
                buffer: Vec::new(),
            };
            loop {
                match reader.read_message() {
                    Ok(Some(value)) => {
                        if sender.send(WireEvent::Message(value)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(err) => {
                        let _ = sender.send(WireEvent::Failed(err));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin: Some(stdin),
            events,
            next_id: 1,
        })
    }

    fn send_frame(&mut self, body: &[u8]) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "stdin is already closed".to_string())?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        stdin
            .write_all(header.as_bytes())
            .and_then(|()| stdin.write_all(body))
            .and_then(|()| stdin.flush())
            .map_err(|err| format!("writing frame to server stdin: {err}"))
    }

    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.send_frame(message.to_string().as_bytes())?;
        self.await_response(id, RESPONSE_TIMEOUT)
    }

    fn notify(&mut self, method: &str, params: Option<serde_json::Value>) -> Result<(), String> {
        let mut message = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
        });
        if let Some(params) = params {
            message["params"] = params;
        }
        self.send_frame(message.to_string().as_bytes())
    }

    /// Returns the response correlated to `id`, skipping server-to-client
    /// notifications. Any other response id is a protocol violation.
    fn await_response(&mut self, id: u64, timeout: Duration) -> Result<serde_json::Value, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let message = self.await_message(deadline, &format!("response id {id}"))?;
            let is_response = message.get("result").is_some() || message.get("error").is_some();
            if !is_response {
                continue;
            }
            if message.get("id").and_then(serde_json::Value::as_u64) == Some(id) {
                return Ok(message);
            }
            return Err(format!(
                "protocol violation: received response with id {:?} while awaiting id {id}: {message}",
                message.get("id")
            ));
        }
    }

    /// Returns the next message of any kind before `deadline`.
    fn await_message(
        &mut self,
        deadline: Instant,
        waiting_for: &str,
    ) -> Result<serde_json::Value, String> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| format!("timed out waiting for {waiting_for}"))?;
        match self.events.recv_timeout(remaining) {
            Ok(WireEvent::Message(message)) => Ok(message),
            Ok(WireEvent::Failed(err)) => Err(format!(
                "stdout framing failed while awaiting {waiting_for}: {err}"
            )),
            Err(RecvTimeoutError::Timeout) => Err(format!("timed out waiting for {waiting_for}")),
            Err(RecvTimeoutError::Disconnected) => Err(format!(
                "server closed stdout (EOF or exit) while awaiting {waiting_for}"
            )),
        }
    }

    fn wait_exit(&mut self, timeout: Duration) -> Result<ExitStatus, String> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "ripr lsp did not exit within {timeout:?} (terminated by Drop)"
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(err) => return Err(format!("try_wait on server process failed: {err}")),
            }
        }
    }
}

impl Drop for SpawnSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A unique temp fixture root, removed on drop.
struct FixtureRoot {
    path: PathBuf,
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn unique_fixture_root(name: &str) -> Result<FixtureRoot, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("ripr-lsp-{name}-{}-{stamp}", std::process::id()));
    // Install the removal guard before any directory exists, so a partial
    // `create_dir_all` failure or any later `?` in fixture setup cannot leak
    // the temp tree (#7189 review).
    let fixture = FixtureRoot { path: path.clone() };
    fs::create_dir_all(&path).map_err(|err| format!("create fixture root failed: {err}"))?;
    Ok(fixture)
}

/// Minimal `file://` URI for an absolute fixture path.
fn file_uri_for_fixture(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err(format!("fixture root must be absolute: {}", path.display()));
    }
    let text = path
        .to_str()
        .ok_or_else(|| format!("fixture root is not UTF-8: {}", path.display()))?;
    let text = text.replace('\\', "/");
    let absolute = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };
    let encoded = absolute
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F");
    Ok(format!("file://{encoded}"))
}

fn copy_fixture_tree(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target)
        .map_err(|err| format!("create {} failed: {err}", target.display()))?;
    let entries =
        fs::read_dir(source).map_err(|err| format!("read {} failed: {err}", source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read dir entry failed: {err}"))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("read entry type failed: {err}"))?;
        let target_path = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_fixture_tree(&entry.path(), &target_path)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target_path)
                .map_err(|err| format!("copy fixture file failed: {err}"))?;
        }
    }
    Ok(())
}

/// The #7170 fixture recipe: a committed copy of the tracked boundary-gap
/// input plus one uncommitted boundary edit, so the degraded refresh must
/// still publish that seeded diff finding's diagnostics. Fixture git runs
/// through the shared deadline-bounded `common::fixture_git` helper
/// (deadline + one idempotent retry + commit reconcile, #7189 review), the
/// same hardened contract every other integration harness uses.
fn seeded_git_fixture_root(name: &str) -> Result<FixtureRoot, String> {
    let fixture = unique_fixture_root(name)?;
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/boundary_gap/input");
    copy_fixture_tree(&source, &fixture.path)?;
    fixture_git_ok(&fixture.path, &["init", "-q"])?;
    fixture_git_ok(&fixture.path, &["add", "-A"])?;
    fixture_git_ok(
        &fixture.path,
        &[
            "-c",
            "user.email=ripr-test@example.com",
            "-c",
            "user.name=ripr-test",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "-qm",
            "fixture baseline",
        ],
    )?;
    fixture_git_ok(&fixture.path, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let seed_path = fixture.path.join("src/lib.rs");
    let seed_source =
        fs::read_to_string(&seed_path).map_err(|err| format!("read seeded lib failed: {err}"))?;
    let seed_changed = seed_source.replace(
        "amount >= discount_threshold",
        "amount > discount_threshold",
    );
    if seed_changed == seed_source {
        return Err("seeded boundary text not found in the fixture lib".to_string());
    }
    fs::write(&seed_path, seed_changed).map_err(|err| format!("write seeded lib failed: {err}"))?;
    Ok(fixture)
}

fn expect_result<'a>(
    response: &'a serde_json::Value,
    method: &str,
) -> Result<&'a serde_json::Value, String> {
    response
        .get("result")
        .ok_or_else(|| format!("expected a result for `{method}`, got: {response}"))
}

/// `initialize` against `root_uri` with the #7170 options, then `initialized`.
fn handshake(session: &mut SpawnSession, root_uri: &str) -> Result<(), String> {
    let initialize = session.request(
        "initialize",
        serde_json::json!({
            "processId": null,
            "rootUri": root_uri,
            "initializationOptions": {
                "baseRef": "HEAD",
                "checkMode": "instant",
                "diagnosticProfile": "full"
            },
            "capabilities": {},
        }),
    )?;
    expect_result(&initialize, "initialize")?;
    session.notify("initialized", Some(serde_json::json!({})))
}

/// Runs one production refresh, collecting every server notification until
/// the refresh response. A response with any other id is a protocol
/// violation and fails the collection.
fn collect_refresh(
    session: &mut SpawnSession,
) -> Result<(Vec<serde_json::Value>, serde_json::Value), String> {
    let id = session.next_id;
    session.next_id += 1;
    session.send_frame(
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "workspace/executeCommand",
            "params": {"command": "ripr.refresh", "arguments": []},
        })
        .to_string()
        .as_bytes(),
    )?;
    let deadline = Instant::now() + ANALYSIS_TIMEOUT;
    let mut notifications = Vec::new();
    loop {
        let message = session.await_message(deadline, "ripr.refresh response")?;
        if message.get("method").is_some() {
            notifications.push(message);
            continue;
        }
        if message.get("id").and_then(serde_json::Value::as_u64) == Some(id) {
            if message.get("error").is_some() {
                return Err(format!("refresh command failed: {message}"));
            }
            return Ok((notifications, message));
        }
        return Err(format!(
            "protocol violation: received response with id {:?} while awaiting refresh id {id}: {message}",
            message.get("id")
        ));
    }
}

fn analysis_status_params(notifications: &[serde_json::Value]) -> Vec<serde_json::Value> {
    notifications
        .iter()
        .filter(|message| {
            message.get("method").and_then(serde_json::Value::as_str) == Some("ripr/analysisStatus")
        })
        .map(|message| message["params"].clone())
        .collect()
}

fn log_messages_of_type(notifications: &[serde_json::Value], message_type: u64) -> Vec<String> {
    notifications
        .iter()
        .filter(|message| {
            message.get("method").and_then(serde_json::Value::as_str) == Some("window/logMessage")
                && message["params"]["type"].as_u64() == Some(message_type)
        })
        .filter_map(|message| message["params"]["message"].as_str().map(str::to_string))
        .collect()
}

/// The diff-finding diagnostics (`code`, 0-based start line, message) in
/// the LAST `textDocument/publishDiagnostics` for exactly `fixture_uri`.
/// LSP publishes replace rather than merge: the client displays only the
/// final publish for a URI, so the retention oracle must judge that publish
/// alone — a merged-history oracle credits a finding a later publish
/// cleared, and a URI suffix admits another workspace's `src/lib.rs`
/// (retention-oracle defect confirmed by independent review on #7189).
/// Finding codes are the verbatim `ExposureClass` labels; seam and gap
/// diagnostics carry `ripr-`-prefixed codes and never match, so this pins
/// the seeded finding itself rather than the pre-existing seam diagnostic
/// the clean baseline also publishes (#7170 review).
fn lib_finding_diagnostics(
    fixture_uri: &str,
    notifications: &[serde_json::Value],
) -> Vec<(String, u64, String)> {
    let Some(message) = notifications.iter().rev().find(|message| {
        message.get("method").and_then(serde_json::Value::as_str)
            == Some("textDocument/publishDiagnostics")
            && message["params"]["uri"].as_str() == Some(fixture_uri)
    }) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if let Some(diagnostics) = message["params"]["diagnostics"].as_array() {
        for diagnostic in diagnostics {
            let code = diagnostic
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let line = diagnostic["range"]["start"]["line"].as_u64();
            let text = diagnostic
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if FINDING_CODES.contains(&code)
                && let Some(line) = line
            {
                found.push((code.to_string(), line, text.to_string()));
            }
        }
    }
    found.sort();
    found
}

/// Synthetic publish for the oracle-integrity controls below.
fn publish_diagnostics(uri: &str, diagnostics: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {"uri": uri, "diagnostics": diagnostics},
    })
}

/// A publish carrying one seeded finding diagnostic for `uri`.
fn finding_publish(uri: &str) -> serde_json::Value {
    publish_diagnostics(
        uri,
        serde_json::json!([
            {
                "code": "exposed",
                "range": {"start": {"line": SEEDED_LINE}},
                "message": "boundary change exposed",
            }
        ]),
    )
}

/// A publish clearing every diagnostic for `uri`.
fn empty_publish(uri: &str) -> serde_json::Value {
    publish_diagnostics(uri, serde_json::json!([]))
}

/// The oracle must fail retention when the final publish for the fixture URI
/// clears the finding: the merged-history oracle returned the earlier
/// finding and let an editor whose final state is empty pass (#7189 review).
#[test]
fn retention_oracle_fails_when_a_later_publish_clears_the_finding() -> Result<(), String> {
    let findings = lib_finding_diagnostics(
        CONTROL_URI,
        &[finding_publish(CONTROL_URI), empty_publish(CONTROL_URI)],
    );
    if !findings.is_empty() {
        return Err(format!(
            "a later empty publish for the fixture URI must fail retention: {findings:?}"
        ));
    }
    Ok(())
}

/// The oracle must pass retention when the final publish keeps the finding.
#[test]
fn retention_oracle_passes_when_the_final_publish_keeps_the_finding() -> Result<(), String> {
    let findings = lib_finding_diagnostics(CONTROL_URI, &[finding_publish(CONTROL_URI)]);
    let expected = vec![(
        "exposed".to_string(),
        SEEDED_LINE,
        "boundary change exposed".to_string(),
    )];
    if findings != expected {
        return Err(format!(
            "the kept finding must pass retention: {findings:?}"
        ));
    }
    Ok(())
}

/// A publish for another workspace's `src/lib.rs` must never satisfy the
/// fixture URI's retention, while that foreign URI's own oracle still
/// selects its final publish (#7189 review).
#[test]
fn retention_oracle_ignores_a_foreign_uri_publish() -> Result<(), String> {
    let findings = lib_finding_diagnostics(
        CONTROL_URI,
        &[finding_publish(FOREIGN_URI), empty_publish(CONTROL_URI)],
    );
    if !findings.is_empty() {
        return Err(format!(
            "a foreign-URI finding must not pass the fixture URI's retention: {findings:?}"
        ));
    }
    let kept = lib_finding_diagnostics(FOREIGN_URI, &[finding_publish(FOREIGN_URI)]);
    let expected = vec![(
        "exposed".to_string(),
        SEEDED_LINE,
        "boundary change exposed".to_string(),
    )];
    if kept != expected {
        return Err(format!(
            "the foreign URI's own oracle must still work: {kept:?}"
        ));
    }
    Ok(())
}

fn shutdown_exit_and_wait(session: &mut SpawnSession) -> Result<(), String> {
    let _ = session.request("shutdown", serde_json::Value::Null)?;
    session.notify("exit", None)?;
    let status = session.wait_exit(EXIT_TIMEOUT)?;
    if status.success() {
        return Ok(());
    }
    Err(format!(
        "expected exit code 0 after `shutdown` then `exit`, got: {status}"
    ))
}

/// An invalid `RIPR_REPO_EXPOSURE_SEAM_LIMIT` in the spawned server's env
/// degrades the production refresh on the wire (#7147, RIPR-SPEC-0141):
/// the `seam_inventory` outcome is `failed`/`seam_inventory_failed`, the
/// run leaves `full`, the client sees exactly one WARNING naming the
/// component and its recovery, and the seeded diff finding still publishes.
/// The control spawn without the override must report none of this, proving
/// the test observes the forcing rather than a fixture artifact.
#[test]
fn spawned_server_with_invalid_seam_limit_degrades_seam_inventory_through_production_refresh()
-> Result<(), String> {
    let fixture = seeded_git_fixture_root("seam-degradation-spawn")?;
    let root_uri = file_uri_for_fixture(&fixture.path)?;
    // The exact URI the server must publish the seeded lib finding under;
    // the retention oracle matches it exactly (#7189 review).
    let fixture_lib_uri = file_uri_for_fixture(&fixture.path.join("src").join("lib.rs"))?;

    // Control: without the override the same fixture must refresh `full`
    // with a `complete` seam inventory and no degradation warning.
    let control_findings = {
        let mut session = SpawnSession::spawn(None)?;
        handshake(&mut session, &root_uri)?;
        let (notifications, refresh) = collect_refresh(&mut session)?;
        expect_result(&refresh, "ripr.refresh")?;
        let statuses = analysis_status_params(&notifications);
        let Some(status) = statuses.last() else {
            return Err("control refresh published no analysis status".to_string());
        };
        if status["run_status"].as_str() != Some("full") {
            return Err(format!("control run must be full, got: {status}"));
        }
        let components = status["components"]
            .as_array()
            .ok_or_else(|| format!("control status must expose typed components: {status}"))?;
        let seam_outcomes: Vec<_> = components
            .iter()
            .filter(|outcome| outcome["component"].as_str() == Some("seam_inventory"))
            .collect();
        if seam_outcomes.len() != 1 {
            return Err(format!(
                "control components must include exactly one seam_inventory outcome: {components:?}"
            ));
        }
        if seam_outcomes[0]["state"].as_str() != Some("complete") {
            return Err(format!(
                "control seam inventory must be complete: {}",
                seam_outcomes[0]
            ));
        }
        let control_warnings = log_messages_of_type(&notifications, 2);
        if control_warnings
            .iter()
            .any(|message| message.contains("seam_inventory failed"))
        {
            return Err(format!(
                "control run must not warn about seam degradation: {control_warnings:?}"
            ));
        }
        let findings = lib_finding_diagnostics(&fixture_lib_uri, &notifications);
        if !findings.iter().any(|(_, line, _)| *line == SEEDED_LINE) {
            return Err(format!(
                "control run must publish the seeded finding diagnostic at line {SEEDED_LINE} for {fixture_lib_uri}: {findings:?}"
            ));
        }
        shutdown_exit_and_wait(&mut session)?;
        findings
    };

    // Forced run: the invalid child-env override fails the seam inventory
    // through the production refresh path.
    let mut session = SpawnSession::spawn(Some(INVALID_SEAM_LIMIT))?;
    handshake(&mut session, &root_uri)?;
    let (notifications, refresh) = collect_refresh(&mut session)?;
    expect_result(&refresh, "ripr.refresh")?;
    let statuses = analysis_status_params(&notifications);
    let Some(status) = statuses.last() else {
        return Err("degraded refresh published no analysis status".to_string());
    };
    if status["run_status"].as_str() != Some("limited") {
        return Err(format!(
            "an invalid seam limit must limit the run, got: {status}"
        ));
    }
    let components = status["components"]
        .as_array()
        .ok_or_else(|| format!("status must expose typed components: {status}"))?;
    let seam_outcomes: Vec<_> = components
        .iter()
        .filter(|outcome| outcome["component"].as_str() == Some("seam_inventory"))
        .collect();
    // The single shared inventory call records exactly one outcome: a wiring
    // change that appends it twice must fail, not pass on `find`.
    if seam_outcomes.len() != 1 {
        return Err(format!(
            "components must include exactly one seam_inventory outcome: {components:?}"
        ));
    }
    let seam = seam_outcomes[0];
    if seam["state"].as_str() != Some("failed")
        || seam["kind"].as_str() != Some("seam_inventory_failed")
        || seam["findings_trustworthy"].as_bool() != Some(true)
        || seam["snapshot_identity"].is_null()
    {
        return Err(format!("unexpected seam_inventory outcome: {seam}"));
    }
    let recovery = seam["recovery"].as_str().unwrap_or("");
    if !recovery.contains("ripr.refreshDiagnostics") {
        return Err(format!(
            "the degraded outcome must name a concrete recovery route: {seam}"
        ));
    }
    // The outcome message must name the forcing variable and value: a
    // different inventory failure (a cache-limit parse error, a walk error)
    // must fail here, not pass as the forced degradation.
    let detail = seam["message"].as_str().unwrap_or("");
    if !detail.contains("seam diagnostics skipped")
        || !detail.contains(SEAM_LIMIT_ENV)
        || !detail.contains(INVALID_SEAM_LIMIT)
    {
        return Err(format!(
            "the degraded outcome must name the invalid seam-limit override: {seam}"
        ));
    }
    let diff = components
        .iter()
        .find(|outcome| outcome["component"].as_str() == Some("diff"));
    if diff.and_then(|outcome| outcome["state"].as_str()) != Some("complete") {
        return Err(format!(
            "ordinary diff findings must stay complete and disclosed: {components:?}"
        ));
    }
    let warnings = log_messages_of_type(&notifications, 2);
    let degradation_warnings = warnings
        .iter()
        .filter(|message| message.contains("seam_inventory failed"))
        .count();
    if degradation_warnings != 1 {
        return Err(format!(
            "expected exactly one degradation warning, got {degradation_warnings}: {warnings:?}"
        ));
    }
    if !warnings
        .iter()
        .any(|message| message.contains("seam_inventory failed") && message.contains("recovery:"))
    {
        return Err(format!(
            "the degradation warning must name the recovery route: {warnings:?}"
        ));
    }
    let degraded_findings = lib_finding_diagnostics(&fixture_lib_uri, &notifications);
    if !degraded_findings
        .iter()
        .any(|(_, line, _)| *line == SEEDED_LINE)
    {
        return Err(format!(
            "the degraded refresh must still publish the seeded finding diagnostic at line {SEEDED_LINE}: {degraded_findings:?}"
        ));
    }
    // The degraded refresh must have published every finding diagnostic the
    // control run publishes: degradation suppresses nothing, it only limits
    // the run status.
    let suppressed: Vec<_> = control_findings
        .iter()
        .filter(|finding| !degraded_findings.contains(finding))
        .collect();
    if !suppressed.is_empty() {
        return Err(format!(
            "the degraded refresh suppressed finding diagnostics: {suppressed:?}"
        ));
    }
    shutdown_exit_and_wait(&mut session)
}
