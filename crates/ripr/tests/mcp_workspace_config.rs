//! #6825: the MCP session must honor the workspace's own configuration —
//! the same `load_for_root` resolution the CLI uses — so a Python-enabled
//! workspace produces its finding through `ripr_refresh` / `ripr_list_gaps`
//! / `ripr_get_gap`, a config-less Rust workspace keeps the built-in
//! defaults posture, and a present-but-unparseable `ripr.toml` fails the
//! refresh attempt closed with `config_invalid`.
//!
//! Python needs the `lang-python` feature (default); a build without it
//! refuses a Python project before any parse runs, so neither the loaded
//! config posture nor its absence is observable there.

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// One bounded sequential MCP session over the real binary: every request
/// line is written only after the previous reply arrived, so refreshes and
/// evidence reads interleave without oversized-pipe deadlocks.
struct Session {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    replies: std::sync::mpsc::Receiver<Value>,
    stdout: Option<std::thread::JoinHandle<Vec<u8>>>,
    stderr: Option<std::thread::JoinHandle<Vec<u8>>>,
    next_id: u64,
}

impl Session {
    fn spawn(root: &Path) -> Result<Self, String> {
        Self::spawn_with_env(root, &[])
    }

    /// Spawn with extra server environment, as a host's `env` block sets it.
    fn spawn_with_env(root: &Path, env: &[(&str, &str)]) -> Result<Self, String> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command.env_remove("RIPR_GIT_TIMEOUT");
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command
            .args(["mcp", "--stdio", "--root"])
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("spawn ripr mcp: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "spawned MCP process did not expose stdin".to_string())?;
        let mut stdout_pipe = child
            .stdout
            .take()
            .ok_or_else(|| "spawned MCP process did not expose stdout".to_string())?;
        let mut stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| "spawned MCP process did not expose stderr".to_string())?;
        // Parse replies on the reader thread; notifications and replies for
        // other ids are skipped by the caller's id match.
        let (sender, receiver) = std::sync::mpsc::channel();
        let stdout = std::thread::spawn(move || {
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(&mut stdout_pipe);
            let mut buffer = Vec::new();
            loop {
                let mut frame = Vec::new();
                match reader.read_until(b'\n', &mut frame) {
                    Ok(0) => break,
                    Err(_) => break,
                    Ok(_) => {
                        if let Ok(value) = serde_json::from_slice::<Value>(&frame) {
                            let _ = sender.send(value);
                        }
                        buffer.extend_from_slice(&frame);
                    }
                }
            }
            buffer
        });
        let stderr = std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stderr_pipe, &mut buffer);
            buffer
        });
        Ok(Self {
            child,
            stdin: Some(stdin),
            replies: receiver,
            stdout: Some(stdout),
            stderr: Some(stderr),
            next_id: 1,
        })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        line.push(b'\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "the session stdin was already released".to_string())?;
        stdin
            .write_all(&line)
            .map_err(|error| format!("write {method}: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("flush {method}: {error}"))?;
        // A request without an id field is a notification: no reply.
        if request.get("id").is_none() {
            return Ok(Value::Null);
        }
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!("timed out waiting for the {method} reply"));
            }
            let reply = self
                .replies
                .recv_timeout(remaining)
                .map_err(|_disconnected| {
                    format!("reply channel closed before the {method} reply")
                })?;
            if reply.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(reply);
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Closing stdin lets the server exit on its own; give an exiting
        // instrumented process time to finish writing its coverage profile
        // before the owned kill, which would otherwise truncate it (#4978).
        self.stdin = None;
        let grace = Instant::now() + Duration::from_secs(2);
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < grace {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(stdout) = self.stdout.take() {
            let _ = stdout.join();
        }
        if let Some(stderr) = self.stderr.take() {
            let _ = stderr.join();
        }
    }
}

fn workspace_name(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ripr-mcp-workspace-config-{label}-{}-{stamp}",
        std::process::id()
    ))
}

fn commit_fixture(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    )
}

fn structured_tool_result(reply: &Value, context: &str) -> Result<Value, String> {
    let result = reply
        .get("result")
        .ok_or_else(|| format!("{context} lost its result: {reply}"))?;
    if result
        .pointer("/isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!("{context} unexpectedly failed: {result}"));
    }
    result
        .get("structuredContent")
        .cloned()
        .ok_or_else(|| format!("{context} omitted structuredContent: {result}"))
}

/// Best-effort fixture custody: the repo's `[env] TEMP=target` redirect puts
/// test fixtures inside the outer git worktree, so leaked git-repo fixtures
/// slow every later git invocation in the tree. The guard removes the root
/// on scope exit, success or failure.
struct FixtureGuard {
    root: PathBuf,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// `ripr_refresh` reports a failed attempt as an `isError: true` envelope whose
/// document carries `attempt.state: failed` with the typed failure — the
/// last-known-good snapshot is kept, so the attempt result is data.
fn refresh_attempt_failure_code(reply: &Value, context: &str) -> Result<String, String> {
    let result = reply
        .get("result")
        .ok_or_else(|| format!("{context} lost its result: {reply}"))?;
    // A failed attempt is data and an error on the wire (#11571).
    if result.pointer("/isError").and_then(Value::as_bool) != Some(true) {
        return Err(format!(
            "{context} must be isError:true with the failed document: {result}"
        ));
    }
    if result
        .pointer("/structuredContent/attempt/state")
        .and_then(Value::as_str)
        != Some("failed")
    {
        return Err(format!(
            "{context} must report the failed attempt state: {result}"
        ));
    }
    result
        .pointer("/structuredContent/attempt/failure/code")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{context} lost its attempt failure code: {result}"))
}

/// The issue's exact scenario fixture: a Python workspace (`pyproject.toml`
/// plus a `ripr.toml` enabling python) whose candidate commit applies the
/// `>` → `>=` predicate mutation. The CLI scores one finding for this
/// crate; the MCP loop must see the same canonical item.
#[cfg(feature = "lang-python")]
fn write_python_workspace() -> Result<PathBuf, String> {
    let root = workspace_name("python");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"pricing\"\nversion = \"0.1.0\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"python\"]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/pricing.py"),
        "def apply_discount(amount, threshold):\n    if amount > threshold:\n        return amount - 1\n    return amount\n",
    )
    .map_err(|error| error.to_string())?;
    fixture_git_ok(&root, &["init", "--quiet", "--initial-branch=main"])?;
    commit_fixture(&root, "base")?;
    // The candidate branch carries the committed `>` → `>=` mutation; the
    // session diffs the checked-out head against the default branch.
    fixture_git_ok(&root, &["checkout", "-q", "-b", "candidate"])?;
    std::fs::write(
        root.join("src/pricing.py"),
        "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 1\n    return amount\n",
    )
    .map_err(|error| error.to_string())?;
    commit_fixture(&root, "candidate")?;
    Ok(root)
}

/// The control shape: the identical committed-diff mutation on a Rust twin
/// with no `ripr.toml` anywhere. The refresh analysis and its documents
/// must keep the built-in defaults posture (no config file exists).
fn write_rust_workspace() -> Result<PathBuf, String> {
    let root = workspace_name("rust");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"rust-twin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/lib.rs"), "pub const THRESHOLD: u32 = 1;\n")
        .map_err(|error| error.to_string())?;
    fixture_git_ok(&root, &["init", "--quiet", "--initial-branch=main"])?;
    commit_fixture(&root, "base")?;
    fixture_git_ok(&root, &["checkout", "-q", "-b", "candidate"])?;
    std::fs::write(root.join("src/lib.rs"), "pub const THRESHOLD: u32 = 2;\n")
        .map_err(|error| error.to_string())?;
    commit_fixture(&root, "candidate")?;
    Ok(root)
}

fn initialize_session(session: &mut Session) -> Result<(), String> {
    let reply = session.call(
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "ripr-integration-test", "version": "1" }
        }),
    )?;
    if reply
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
        != Some("2025-11-25")
    {
        return Err(format!("initialize did not negotiate: {reply}"));
    }
    session.call("notifications/initialized", json!({}))?;
    Ok(())
}

/// Python-only (#4252): a build without `lang-python` refuses the enabled
/// language before any parse runs, so the loaded posture and the python
/// canonical gap are not observable there; the rust-only controls below
/// still run in that configuration.
#[cfg(feature = "lang-python")]
#[test]
fn python_workspace_analyzes_over_mcp_and_projects_the_loaded_config() -> Result<(), String> {
    let root = write_python_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };
    let mut session = Session::spawn(&root)?;
    initialize_session(&mut session)?;

    // The status document must disclose the loaded config posture with its
    // identity and the enabled language — never a rust-only default.
    let status = session.call(
        "tools/call",
        json!({ "name": "ripr_workspace_status", "arguments": {} }),
    )?;
    let status = structured_tool_result(&status, "workspace_status")?;
    let languages = status
        .pointer("/session/profile/languages")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("status lost the profile languages: {status}"))?;
    let languages: Vec<&str> = languages.iter().filter_map(Value::as_str).collect();
    if !languages.contains(&"python") {
        return Err(format!(
            "a python-enabled workspace must disclose python in the profile: {languages:?}"
        ));
    }
    if status
        .pointer("/session/profile/project_config")
        .and_then(Value::as_str)
        != Some("loaded")
    {
        return Err(format!(
            "a loadable ripr.toml must project the loaded posture: {}",
            status.pointer("/session/profile").unwrap_or(&Value::Null)
        ));
    }
    if status
        .pointer("/session/profile/config_identity")
        .and_then(Value::as_str)
        .is_none_or(|identity| !identity.starts_with("fnv1a64:"))
    {
        return Err(format!(
            "a loaded profile must publish its config identity: {}",
            status.pointer("/session/profile").unwrap_or(&Value::Null)
        ));
    }
    if status
        .pointer("/workspace/configuration/project_config_state")
        .and_then(Value::as_str)
        != Some("loaded")
    {
        return Err("workspace configuration must project loaded".to_string());
    }
    if status
        .pointer("/workspace/configuration/project_config_identity")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err("workspace configuration lost its identity".to_string());
    }

    // The prescribed loop must now surface the Python finding.
    let refresh = session.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    let refresh = structured_tool_result(&refresh, "ripr_refresh")?;
    if refresh
        .pointer("/snapshot/finding_count")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Err(format!(
            "the committed diff must score exactly one finding over MCP: {refresh}"
        ));
    }
    if refresh
        .pointer("/snapshot/outcome_kind")
        .and_then(Value::as_str)
        != Some("complete_with_findings")
    {
        return Err(format!(
            "the python finding must complete the snapshot: {refresh}"
        ));
    }

    let list = session.call(
        "tools/call",
        json!({ "name": "ripr_list_gaps", "arguments": {} }),
    )?;
    let list = structured_tool_result(&list, "ripr_list_gaps")?;
    const CANONICAL_ID: &str =
        "gap:python:src/pricing.py:apply_discount:predicate_boundary:predicate:amount>=threshold";
    if list.pointer("/total").and_then(Value::as_u64) != Some(1)
        || list
            .pointer("/items/0/canonical_id")
            .and_then(Value::as_str)
            != Some(CANONICAL_ID)
    {
        return Err(format!(
            "list_gaps must carry the canonical python gap: {list}"
        ));
    }

    let gap = session.call(
        "tools/call",
        json!({ "name": "ripr_get_gap", "arguments": { "canonical_id": CANONICAL_ID } }),
    )?;
    let gap = structured_tool_result(&gap, "ripr_get_gap")?;
    if gap
        .pointer("/item/causal_attribution/normalized_discriminator")
        .and_then(Value::as_str)
        != Some("amount>=threshold")
    {
        return Err(format!(
            "get_gap must bind the normalized discriminator: {gap}"
        ));
    }

    Ok(())
}

#[test]
fn config_less_rust_workspace_keeps_the_defaults_posture_and_its_analysis() -> Result<(), String> {
    let root = write_rust_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };
    let mut session = Session::spawn(&root)?;
    initialize_session(&mut session)?;

    let status = session.call(
        "tools/call",
        json!({ "name": "ripr_workspace_status", "arguments": {} }),
    )?;
    let status = structured_tool_result(&status, "workspace_status")?;
    let profile = status
        .pointer("/session/profile")
        .ok_or_else(|| format!("status lost its profile: {status}"))?;
    let languages: Vec<String> = profile
        .pointer("/languages")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("profile lost its languages: {profile}"))?
        .iter()
        .filter_map(|language| language.as_str().map(str::to_string))
        .collect();
    if languages != vec!["rust".to_string()] {
        return Err(format!(
            "a config-less rust workspace must keep the rust-only profile: {profile}"
        ));
    }
    if profile.pointer("/project_config").and_then(Value::as_str) != Some("built_in_defaults") {
        return Err(format!(
            "no ripr.toml means the defaults posture, not detected_not_loaded: {profile}"
        ));
    }
    if profile.get("config_identity").is_some() {
        return Err(format!(
            "a defaults run must not claim a config identity: {profile}"
        ));
    }
    if status
        .pointer("/workspace/configuration/project_config_state")
        .and_then(Value::as_str)
        != Some("built_in_defaults_only")
    {
        return Err("workspace configuration must stay built_in_defaults_only".to_string());
    }

    // The analysis itself is unchanged: the committed rust mutation still
    // scores its finding through the same shared authority.
    let refresh = session.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    let refresh = structured_tool_result(&refresh, "ripr_refresh")?;
    if refresh
        .pointer("/snapshot/finding_count")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Err(format!(
            "the rust twin must keep scoring its finding over MCP: {refresh}"
        ));
    }

    Ok(())
}

#[test]
fn unreadable_ripr_toml_fails_the_refresh_closed_with_config_invalid() -> Result<(), String> {
    let root = write_rust_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };
    std::fs::write(root.join("ripr.toml"), "not valid toml =\n")
        .map_err(|error| error.to_string())?;
    let mut session = Session::spawn(&root)?;
    initialize_session(&mut session)?;

    let status = session.call(
        "tools/call",
        json!({ "name": "ripr_workspace_status", "arguments": {} }),
    )?;
    let status = structured_tool_result(&status, "workspace_status")?;
    if status
        .pointer("/session/profile/project_config")
        .and_then(Value::as_str)
        != Some("detected_not_loaded")
    {
        return Err(format!(
            "an unparseable ripr.toml must project detected_not_loaded: {}",
            status.pointer("/session/profile").unwrap_or(&Value::Null)
        ));
    }

    let refresh = session.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    let code = refresh_attempt_failure_code(&refresh, "ripr_refresh")?;
    if code != "config_invalid" {
        return Err(format!(
            "an unparseable ripr.toml must fail the attempt with config_invalid, got {code}"
        ));
    }

    // #14451: the detail is the source-free summary the CLI JSON uses: the
    // location and the parser's reason, never the TOML source excerpt, the
    // CLI-only "fix the error above" trailer, or a mid-word cut.
    std::fs::write(root.join("ripr.toml"), "[canary_table_14451]\nmode = 1\n")
        .map_err(|error| error.to_string())?;
    let refresh = session.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    refresh_attempt_failure_code(&refresh, "ripr_refresh unknown table")?;
    let detail = refresh
        .pointer("/result/structuredContent/attempt/failure/detail")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !detail.contains("TOML parse error at line 1, column 2")
        || !detail.contains("; unknown key, expected one of `analysis`")
        || detail.contains("canary_table_14451")
        || detail.contains('\n')
        || detail.contains("Fix the error above")
        || detail.ends_with('\u{2026}')
    {
        return Err(format!(
            "config_invalid detail must be the source-free summary: {detail:?}"
        ));
    }

    Ok(())
}

/// #6021: the paging inputs are part of the wire contract. A client pages
/// `ripr_list_gaps` with `offset`/`limit`, the document discloses the
/// window (`page.returned`/`has_more`/`next_offset`), and an offset past
/// the selection is an empty disclosed page — never an unknown-argument
/// rejection and never an approved listing dying `result_too_large` on the
/// wire.
#[test]
fn list_gaps_pages_over_the_wire_with_offset_and_limit() -> Result<(), String> {
    let root = write_rust_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };
    let mut session = Session::spawn(&root)?;
    initialize_session(&mut session)?;

    let refresh = session.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    let refresh = structured_tool_result(&refresh, "ripr_refresh")?;
    if refresh
        .pointer("/snapshot/finding_count")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Err(format!(
            "the rust twin must score exactly one finding: {refresh}"
        ));
    }

    // An explicit one-item window is accepted and echoed.
    let paged = session.call(
        "tools/call",
        json!({
            "name": "ripr_list_gaps",
            "arguments": { "offset": 0, "limit": 1 }
        }),
    )?;
    let paged = structured_tool_result(&paged, "ripr_list_gaps")?;
    if paged.pointer("/selected").and_then(Value::as_u64) != Some(1) {
        return Err(format!("the selection must hold the one finding: {paged}"));
    }
    let items = paged
        .pointer("/items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("paged list lost its items: {paged}"))?;
    if items.len() != 1 {
        return Err(format!("limit=1 must return exactly one summary: {paged}"));
    }
    if paged.pointer("/page/limit").and_then(Value::as_u64) != Some(1)
        || paged.pointer("/page/offset").and_then(Value::as_u64) != Some(0)
        || paged.pointer("/page/returned").and_then(Value::as_u64) != Some(1)
        || paged.pointer("/page/has_more").and_then(Value::as_bool) != Some(false)
        || paged.pointer("/page/next_offset") != Some(&Value::Null)
    {
        return Err(format!("the page window must be disclosed: {paged}"));
    }

    // The default call ships the whole (fitting) selection and closes its
    // own window.
    let full = session.call(
        "tools/call",
        json!({ "name": "ripr_list_gaps", "arguments": {} }),
    )?;
    let full = structured_tool_result(&full, "ripr_list_gaps")?;
    let full_items = full
        .pointer("/items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("default list lost its items: {full}"))?;
    if full_items.len() != 1
        || full.pointer("/page/has_more").and_then(Value::as_bool) != Some(false)
    {
        return Err(format!("a fitting selection must ship whole: {full}"));
    }
    if full_items[0] != items[0] {
        return Err(format!(
            "paged and default listings must agree on identity: {paged} vs {full}"
        ));
    }

    // An offset past the selection is an empty final page, not an error.
    let beyond = session.call(
        "tools/call",
        json!({ "name": "ripr_list_gaps", "arguments": { "offset": 5 } }),
    )?;
    let beyond = structured_tool_result(&beyond, "ripr_list_gaps")?;
    if beyond
        .pointer("/items")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        != Some(false)
        || beyond.pointer("/page/returned").and_then(Value::as_u64) != Some(0)
        || beyond.pointer("/page/has_more").and_then(Value::as_bool) != Some(false)
    {
        return Err(format!(
            "an offset past the selection must be an empty disclosed page: {beyond}"
        ));
    }
    Ok(())
}

/// #14386: `ripr_refresh` reads `RIPR_GIT_TIMEOUT` from the server's
/// environment through the same parser as `ripr check`. An invalid value
/// fails the attempt closed with `config_invalid` naming the variable (it
/// used to be ignored, so MCP silently ran a different deadline than the
/// CLI), and a valid value (`0` disables the deadline) still analyzes.
#[test]
fn refresh_honors_ripr_git_timeout_from_the_server_environment() -> Result<(), String> {
    let root = write_rust_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };

    let mut invalid = Session::spawn_with_env(&root, &[("RIPR_GIT_TIMEOUT", "soon")])?;
    initialize_session(&mut invalid)?;
    let refresh = invalid.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    let code = refresh_attempt_failure_code(&refresh, "ripr_refresh")?;
    if code != "config_invalid" {
        return Err(format!(
            "an invalid RIPR_GIT_TIMEOUT must fail the attempt with config_invalid, got {code}"
        ));
    }
    let failure = refresh
        .pointer("/result/structuredContent/attempt/failure")
        .cloned()
        .unwrap_or(Value::Null);
    let detail = failure.get("detail").and_then(Value::as_str).unwrap_or("");
    if !detail.contains("RIPR_GIT_TIMEOUT") || !detail.contains("\"soon\"") {
        return Err(format!(
            "the refusal must name the variable and the rejected value: {failure}"
        ));
    }
    let recovery = failure
        .get("recovery")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !recovery.contains("restart the server") {
        return Err(format!(
            "the recovery must say a fix needs a server restart: {failure}"
        ));
    }
    drop(invalid);

    let mut disabled = Session::spawn_with_env(&root, &[("RIPR_GIT_TIMEOUT", "0")])?;
    initialize_session(&mut disabled)?;
    let refresh = disabled.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    let refresh = structured_tool_result(&refresh, "ripr_refresh")?;
    if refresh
        .pointer("/snapshot/finding_count")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Err(format!(
            "RIPR_GIT_TIMEOUT=0 disables the deadline and keeps the analysis: {refresh}"
        ));
    }
    Ok(())
}

/// #14386: a finite nondefault `RIPR_GIT_TIMEOUT` reaches the git invocations
/// `ripr_refresh` runs, not only the parser. Every `git` on the server's
/// `PATH` is a shim that sleeps 3 s, so under `RIPR_GIT_TIMEOUT=1` the first
/// git call the refresh makes is cut at the 1000 ms deadline and the failure
/// names it. If the refresh substituted the 300 s default for the configured
/// value, the shim would finish its sleep instead and no 1000 ms deadline
/// could appear. This reaches the findings producer only: with every git call
/// cut, the refresh fails before the committed-source probe runs. The probe
/// takes the same resolved value from `run_check` by construction and is not
/// separately exercised here.
#[cfg(unix)]
#[test]
fn refresh_cuts_git_at_the_configured_ripr_git_timeout() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let root = write_rust_workspace()?;
    let _guard = FixtureGuard { root: root.clone() };
    let shim_dir = root.with_file_name(format!(
        "{}-git-shim",
        root.file_name()
            .and_then(|name| name.to_str())
            .ok_or("fixture root must have a file name")?
    ));
    let _shim_guard = FixtureGuard {
        root: shim_dir.clone(),
    };
    std::fs::create_dir_all(&shim_dir).map_err(|error| error.to_string())?;
    let shim = shim_dir.join("git");
    std::fs::write(&shim, "#!/bin/sh\nexec sleep 3\n").map_err(|error| error.to_string())?;
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())?;
    let mut paths = vec![shim_dir.clone()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let search_path = std::env::join_paths(paths)
        .map_err(|error| error.to_string())?
        .to_string_lossy()
        .into_owned();

    let mut session = Session::spawn_with_env(
        &root,
        &[("RIPR_GIT_TIMEOUT", "1"), ("PATH", search_path.as_str())],
    )?;
    initialize_session(&mut session)?;
    let started = Instant::now();
    let refresh = session.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    let elapsed = started.elapsed();
    refresh_attempt_failure_code(&refresh, "ripr_refresh under a 1 s git deadline")?;
    let detail = refresh
        .pointer("/result/structuredContent/attempt/failure/detail")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !detail.contains("git_invocation_timeout") || !detail.contains("1000ms deadline") {
        return Err(format!(
            "the refresh must cut git at the configured 1000ms deadline: {refresh}"
        ));
    }
    if elapsed >= Duration::from_secs(30) {
        return Err(format!(
            "a 1 s git deadline must end the refresh promptly, took {elapsed:?}"
        ));
    }
    Ok(())
}

/// #14347: a Rust crate whose changed predicate is only tested from an
/// unchanged integration test file. With unchanged tests indexed (the
/// default) the boundary is `exposed`; with
/// `[analysis] include_unchanged_tests = false` the test is out of the index
/// and the same predicate reads `no_static_path`.
/// The guard is installed before any fallible setup, so a failed write or
/// commit still removes the fixture.
fn write_unchanged_test_workspace() -> Result<(PathBuf, FixtureGuard), String> {
    let root = workspace_name("analysis-table");
    let _ = std::fs::remove_dir_all(&root);
    let guard = FixtureGuard { root: root.clone() };
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"gate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn above(amount: u32, limit: u32) -> bool {\n    amount > limit\n}\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("tests/above.rs"),
        "use gate::above;\n\n#[test]\nfn boundary_is_exclusive() {\n    assert!(!above(2, 2));\n    assert!(above(3, 2));\n}\n",
    )
    .map_err(|error| error.to_string())?;
    fixture_git_ok(&root, &["init", "--quiet", "--initial-branch=main"])?;
    commit_fixture(&root, "base")?;
    fixture_git_ok(&root, &["checkout", "-q", "-b", "candidate"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn above(amount: u32, limit: u32) -> bool {\n    amount >= limit\n}\n",
    )
    .map_err(|error| error.to_string())?;
    commit_fixture(&root, "candidate")?;
    Ok((root, guard))
}

/// The sorted exposure classes `ripr_list_gaps` reports after one refresh,
/// with the session's disclosed profile and desired-input modes.
fn mcp_classes_and_modes(root: &Path) -> Result<(Vec<String>, String, String), String> {
    let mut session = Session::spawn(root)?;
    initialize_session(&mut session)?;
    let status = session.call(
        "tools/call",
        json!({ "name": "ripr_workspace_status", "arguments": {} }),
    )?;
    let status = structured_tool_result(&status, "workspace_status")?;
    let profile_mode = status
        .pointer("/session/profile/mode")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("status lost the profile mode: {status}"))?
        .to_string();
    let desired_mode = status
        .pointer("/session/current_desired_input/mode")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("status lost the desired-input mode: {status}"))?
        .to_string();
    let refresh = session.call(
        "tools/call",
        json!({ "name": "ripr_refresh", "arguments": {} }),
    )?;
    structured_tool_result(&refresh, "ripr_refresh")?;
    let list = session.call(
        "tools/call",
        json!({ "name": "ripr_list_gaps", "arguments": {} }),
    )?;
    let list = structured_tool_result(&list, "ripr_list_gaps")?;
    let mut classes = list
        .pointer("/items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("list_gaps lost its items: {list}"))?
        .iter()
        .map(|item| {
            item.get("class")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("a listed gap lost its class: {item}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    classes.sort();
    Ok((classes, profile_mode, desired_mode))
}

/// The sorted finding classifications `ripr check --format json` reports
/// on the same root, the CLI side of the parity oracle.
fn cli_classes(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--format", "json", "--root"])
        .arg(root)
        .output()
        .map_err(|error| format!("spawn ripr check: {error}"))?;
    let document: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "ripr check did not print JSON ({error}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let mut classes = document
        .pointer("/findings")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("ripr check lost its findings: {document}"))?
        .iter()
        .map(|finding| {
            finding
                .get("classification")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("a CLI finding lost its classification: {finding}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    classes.sort();
    Ok(classes)
}

#[test]
fn refresh_honors_the_analysis_table_like_ripr_check() -> Result<(), String> {
    let (root, _guard) = write_unchanged_test_workspace()?;

    // Control, no ripr.toml: the unchanged test is indexed, so the boundary
    // is exposed, the session runs draft, and MCP agrees with the CLI.
    let (default_classes, profile_mode, desired_mode) = mcp_classes_and_modes(&root)?;
    if !default_classes.iter().any(|class| class == "exposed") {
        return Err(format!(
            "with unchanged tests indexed the boundary must read exposed: {default_classes:?}"
        ));
    }
    if profile_mode != "draft" || desired_mode != "draft" {
        return Err(format!(
            "a config-less session must run draft: profile {profile_mode}, desired {desired_mode}"
        ));
    }
    let cli_default = cli_classes(&root)?;
    if cli_default != default_classes {
        return Err(format!(
            "MCP and ripr check must agree with defaults: {default_classes:?} vs {cli_default:?}"
        ));
    }

    // The repo's [analysis] table now drops unchanged tests and picks a
    // non-draft mode; a fresh session must apply both, exactly as the CLI.
    std::fs::write(
        root.join("ripr.toml"),
        "[analysis]\nmode = \"fast\"\ninclude_unchanged_tests = false\n",
    )
    .map_err(|error| error.to_string())?;
    let (configured_classes, profile_mode, desired_mode) = mcp_classes_and_modes(&root)?;
    if configured_classes.is_empty()
        || configured_classes
            .iter()
            .any(|class| class != "no_static_path")
    {
        return Err(format!(
            "include_unchanged_tests = false must drop the unchanged test's reach over MCP: {configured_classes:?}"
        ));
    }
    if profile_mode != "fast" || desired_mode != "fast" {
        return Err(format!(
            "the session must disclose the configured mode: profile {profile_mode}, desired {desired_mode}"
        ));
    }
    let cli_configured = cli_classes(&root)?;
    if cli_configured != configured_classes {
        return Err(format!(
            "MCP and ripr check must agree under ripr.toml: {configured_classes:?} vs {cli_configured:?}"
        ));
    }

    // The mode alone must reach the analysis, not only the disclosed label:
    // `instant` indexes changed files only, so with unchanged tests still
    // included the unchanged test drops out and the boundary reads
    // `no_static_path` again.
    std::fs::write(root.join("ripr.toml"), "[analysis]\nmode = \"instant\"\n")
        .map_err(|error| error.to_string())?;
    let (instant_classes, profile_mode, desired_mode) = mcp_classes_and_modes(&root)?;
    if instant_classes.is_empty()
        || instant_classes
            .iter()
            .any(|class| class != "no_static_path")
    {
        return Err(format!(
            "mode = instant must index changed files only over MCP: {instant_classes:?}"
        ));
    }
    if profile_mode != "instant" || desired_mode != "instant" {
        return Err(format!(
            "the session must disclose instant: profile {profile_mode}, desired {desired_mode}"
        ));
    }
    let cli_instant = cli_classes(&root)?;
    if cli_instant != instant_classes {
        return Err(format!(
            "MCP and ripr check must agree under mode = instant: {instant_classes:?} vs {cli_instant:?}"
        ));
    }
    Ok(())
}
