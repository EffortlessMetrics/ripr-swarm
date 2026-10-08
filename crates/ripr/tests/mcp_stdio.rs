use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "common/mod.rs"]
mod common;
use common::fixture_git::fixture_git_ok;

#[path = "support/mcp_stdio_observation.rs"]
mod process_observation;
use process_observation::Observation;

fn workspace_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "crate manifest directory has no workspace parent".to_string())
}

/// One spawn constructor for every worktree-built `ripr` launch in this
/// harness (`mcp --stdio` and `agent card`). Process policy counts
/// constructor sites, not call sites.
fn worktree_ripr() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
}

fn run_mcp(root: &Path, chunks: &[&[u8]]) -> Result<Output, String> {
    run_mcp_with_input_custody(root, chunks, false)
}

fn run_mcp_with_input_custody(
    root: &Path,
    chunks: &[&[u8]],
    hold_input_until_exit: bool,
) -> Result<Output, String> {
    let (executable, executable_sha256) =
        Observation::executable_custody(Path::new(env!("CARGO_BIN_EXE_ripr")))?;
    let launched = Instant::now();
    let mut child = worktree_ripr()
        .args(["mcp", "--stdio", "--root"])
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn ripr mcp: {error}"))?;
    let observation = Arc::new(Observation::new(
        launched,
        executable,
        executable_sha256,
        child.id(),
        hold_input_until_exit,
    ));
    observation.record("child_spawn_returned", None, None);
    let Some(mut stdin) = child.stdin.take() else {
        return Err("spawned MCP process did not expose stdin".to_string());
    };
    // Write requests from a thread and drain both output pipes
    // concurrently: combined replies can exceed the OS pipe capacity
    // (Windows anonymous pipes are small), and a server blocked writing
    // stdout while the harness waits for exit would deadlock the test
    // (#3587 review).
    let stdin_chunks: Vec<Vec<u8>> = chunks.iter().map(|chunk| chunk.to_vec()).collect();
    let (response_sender, response_receiver) = std::sync::mpsc::channel();
    let (input_release, input_retention) = std::sync::mpsc::channel();
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| "spawned MCP process did not expose stdout".to_string())?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| "spawned MCP process did not expose stderr".to_string())?;
    let stdout_observation = observation.clone();
    let stdout_reader = std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(&mut stdout_pipe);
        let mut buffer = Vec::new();
        loop {
            let mut frame = Vec::new();
            match reader.read_until(b'\n', &mut frame) {
                Ok(0) => {
                    stdout_observation.record("stdout_eof", None, None);
                    break;
                }
                Err(error) => {
                    stdout_observation.record("stdout_read_failed", None, Some(error.kind()));
                    break;
                }
                Ok(_) => {
                    stdout_observation.record("stdout_line_read", None, None);
                    if let Ok(value) = serde_json::from_slice::<Value>(&frame) {
                        let _ = response_sender.send(value.get("id").cloned());
                    }
                    buffer.extend_from_slice(&frame);
                }
            }
        }
        buffer
    });
    // SDK EOF terminates service work. Keep stdin alive until each actual
    // response arrives; closing a prewritten script is not a reply oracle.
    let input_observation = observation.clone();
    let writer = std::thread::spawn(move || {
        input_observation.record("input_writer_started", None, None);
        let mut write_script = || {
            let mut pending = Vec::new();
            let mut sent = 0_usize;
            let mut input_line = 0_usize;
            for chunk in &stdin_chunks {
                for byte in chunk {
                    pending.push(*byte);
                    if *byte != b'\n' {
                        continue;
                    }
                    input_line = input_line.saturating_add(1);
                    if let Err(error) = stdin.write_all(pending.get(sent..).unwrap_or_default()) {
                        input_observation.record(
                            "input_line_write_failed",
                            Some(input_line),
                            Some(error.kind()),
                        );
                        return;
                    }
                    input_observation.record("input_line_written", Some(input_line), None);
                    if let Err(error) = stdin.flush() {
                        input_observation.record(
                            "input_line_flush_failed",
                            Some(input_line),
                            Some(error.kind()),
                        );
                        return;
                    }
                    input_observation.record("input_line_flushed", Some(input_line), None);
                    if let Ok(request) = serde_json::from_slice::<Value>(&pending)
                        && let Some(id) = request.get("id")
                    {
                        input_observation.record(
                            "matching_reply_wait_started",
                            Some(input_line),
                            None,
                        );
                        match response_receiver.recv_timeout(Duration::from_secs(10)) {
                            Ok(Some(response_id)) if &response_id == id => {
                                input_observation.record(
                                    "matching_reply_received",
                                    Some(input_line),
                                    None,
                                );
                            }
                            Ok(_) => {
                                input_observation.record(
                                    "reply_id_did_not_match",
                                    Some(input_line),
                                    None,
                                );
                                return;
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                input_observation.record(
                                    "matching_reply_timeout",
                                    Some(input_line),
                                    None,
                                );
                                return;
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                input_observation.record(
                                    "reply_channel_closed",
                                    Some(input_line),
                                    None,
                                );
                                return;
                            }
                        }
                    }
                    pending.clear();
                    sent = 0;
                }
                if let Err(error) = stdin.write_all(pending.get(sent..).unwrap_or_default()) {
                    input_observation.record(
                        "input_fragment_write_failed",
                        None,
                        Some(error.kind()),
                    );
                    return;
                }
                input_observation.record("input_fragment_written", None, None);
                if let Err(error) = stdin.flush() {
                    input_observation.record(
                        "input_fragment_flush_failed",
                        None,
                        Some(error.kind()),
                    );
                    return;
                }
                input_observation.record("input_fragment_flushed", None, None);
                sent = pending.len();
            }
            input_observation.record("input_script_completed", None, None);
        };
        write_script();
        if hold_input_until_exit {
            // Only the parent's observed child exit or owned timeout cleanup
            // releases this pipe. Reply timeout cannot supply a helpful EOF.
            input_observation.record("input_retained_until_parent_release", None, None);
            let _ = input_retention.recv();
        }
        drop(stdin);
        input_observation.record("stdin_released", None, None);
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stderr_pipe, &mut buffer);
        buffer
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    observation.record("owned_deadline_started", None, None);
    let mut status = None;
    let mut timed_out = false;
    loop {
        match child
            .try_wait()
            .map_err(|error| format!("poll ripr mcp: {error}"))?
        {
            Some(exit) => {
                observation.record("child_exit_observed", None, None);
                status = Some(exit);
                break;
            }
            None if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            None => {
                observation.record("owned_deadline_reached", None, None);
                child
                    .kill()
                    .map_err(|error| format!("kill hung ripr mcp: {error}"))?;
                child
                    .wait()
                    .map_err(|error| format!("reap hung ripr mcp: {error}"))?;
                observation.record("child_terminated_and_reaped", None, None);
                timed_out = true;
                break;
            }
        }
    }
    let _ = input_release.send(());
    let _ = writer.join();
    let stdout = stdout_reader
        .join()
        .map_err(|_join_error| "stdout reader panicked")?;
    let stderr = stderr_reader
        .join()
        .map_err(|_join_error| "stderr reader panicked")?;
    let custody = observation.finish(timed_out, status.as_ref().and_then(|exit| exit.code()));
    if timed_out {
        return Err(format!(
            "ripr mcp exceeded its owned process deadline\nstdout:\n{}\nstderr:\n{}\nprocess observation:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
            custody.unwrap_or_else(|error| format!("observation retention failed: {error}"))
        ));
    }
    let _retained_custody = custody?;
    let status = status.ok_or("ripr mcp status was not collected")?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn response_lines(output: &Output) -> Result<Vec<Value>, String> {
    if !output.status.success() {
        return Err(format!(
            "ripr mcp failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if !output.stderr.is_empty() {
        return Err(format!(
            "successful MCP session contaminated stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = std::str::from_utf8(&output.stdout)
        .map_err(|error| format!("MCP stdout is not UTF-8: {error}"))?;
    stdout
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .map_err(|error| format!("MCP stdout line is not JSON-RPC: {error}: {line}"))
        })
        .collect()
}

fn current_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {
            "name": "ripr-integration-test",
            "version": "1"
        }
    })
}

fn line(value: Value) -> Result<Vec<u8>, String> {
    let mut encoded = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
    encoded.push(b'\n');
    Ok(encoded)
}

#[test]
fn readable_request_id_larger_than_output_cap_terminates_without_oversized_reply()
-> Result<(), String> {
    // The escaped ID fits the public input cap but cannot fit the output
    // cap even in a minimal correlated error. A null or omitted substitute
    // would falsely suggest the readable request ID was unknown.
    const INPUT_CAP: usize = 256 * 1024;
    const OUTPUT_CAP: usize = 128 * 1024;
    let root = workspace_root()?;
    let request_id = "\n".repeat(70 * 1024);
    let discover = line(json!({
        "jsonrpc": "2.0",
        "id": "discover",
        "method": "server/discover",
        "params": { "_meta": current_meta() }
    }))?;
    let request = line(json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "tools/list",
        "params": { "_meta": current_meta() }
    }))?;
    if request.len() > INPUT_CAP {
        return Err("giant-ID fixture exceeds the input cap before execution".into());
    }
    let output = run_mcp_with_input_custody(&root, &[&discover, &request], true)?;
    if output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .count()
        != 1
    {
        return Err("giant readable ID emitted an uncorrelated substitute response".into());
    }
    let first = output
        .stdout
        .split(|byte| *byte == b'\n')
        .find(|frame| !frame.is_empty())
        .ok_or_else(|| "no actual discovery reply before the giant-ID request".to_string())?;
    let discovery: Value = serde_json::from_slice(first).map_err(|error| error.to_string())?;
    if discovery.get("id").and_then(Value::as_str) != Some("discover")
        || discovery.get("result").is_none()
    {
        return Err("giant-ID control did not reach a successful actual discovery".into());
    }
    for frame in output.stdout.split(|byte| *byte == b'\n') {
        if frame.len() > OUTPUT_CAP {
            return Err(format!(
                "MCP emitted a {}-byte frame beyond the {OUTPUT_CAP}-byte output cap",
                frame.len()
            ));
        }
    }
    if output.status.success() {
        return Err(
            "uncorrelatable bounded output must terminate with an operational error".into(),
        );
    }
    let stderr = std::str::from_utf8(&output.stderr).map_err(|error| error.to_string())?;
    if stderr.len() > 1024 || !stderr.contains("MCP output limit") {
        return Err("termination must retain a bounded, redacted MCP output-limit reason".into());
    }
    Ok(())
}

#[test]
fn legacy_stdio_lifecycle_lists_and_reads_the_same_bounded_status() -> Result<(), String> {
    let root = workspace_root()?;
    let initialize = line(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "ripr-integration-test", "version": "1" }
        }
    }))?;
    let split = initialize.len() / 2;
    let remaining = [
        line(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resources/list",
            "params": {}
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "ripr_workspace_status",
                "arguments": {}
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "resources/read",
            "params": { "uri": "ripr://workspace/status" }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "ping",
            "params": {}
        }))?,
    ]
    .concat();
    let output = run_mcp(
        &root,
        &[&initialize[..split], &initialize[split..], &remaining],
    )?;
    let responses = response_lines(&output)?;
    if responses.len() != 6 {
        return Err(format!("expected 6 MCP responses, got {}", responses.len()));
    }
    if responses[0]
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
        != Some("2025-11-25")
    {
        return Err("legacy initialize did not negotiate the requested version".to_string());
    }
    if responses[1]
        .pointer("/result/tools/0/name")
        .and_then(Value::as_str)
        != Some("ripr_workspace_status")
    {
        return Err("tools/list did not expose the status tool".to_string());
    }
    if responses[2]
        .pointer("/result/resources/0/uri")
        .and_then(Value::as_str)
        != Some("ripr://workspace/status")
    {
        return Err("resources/list did not expose the status resource".to_string());
    }
    let structured = responses[3]
        .pointer("/result/structuredContent")
        .ok_or_else(|| "tool result omitted structuredContent".to_string())?;
    // A ready workspace needs no recovery text: the document is the only content.
    if responses[3].pointer("/result/content/1").is_some() {
        return Err(format!(
            "ready status must not carry recovery text: {}",
            responses[3]
        ));
    }
    if structured
        .pointer("/workspace/authority/source_edit_capability")
        .and_then(Value::as_str)
        != Some("none")
        || structured
            .pointer("/workspace/authority/verification_execution_capability")
            .and_then(Value::as_str)
            != Some("none")
        || structured
            .pointer("/workspace/authority/mutation_execution_capability")
            .and_then(Value::as_str)
            != Some("none")
        || structured
            .pointer("/workspace/authority/model_provider")
            .and_then(Value::as_str)
            != Some("none")
    {
        return Err("status authority boundary drifted".to_string());
    }
    let resource_text = responses[4]
        .pointer("/result/contents/0/text")
        .and_then(Value::as_str)
        .ok_or_else(|| "resource result omitted JSON text".to_string())?;
    let resource_status: Value = serde_json::from_str(resource_text)
        .map_err(|error| format!("resource status is not JSON: {error}"))?;
    if &resource_status != structured {
        return Err("tool and resource projected different status payloads".to_string());
    }
    let canonical = root
        .canonicalize()
        .map_err(|error| error.to_string())?
        .to_string_lossy()
        .into_owned();
    // Match a JSON string, not a raw substring: `ripr://workspace/status`
    // contains `/workspace` when this checkout's canonical path is that.
    // Serialize so Windows backslashes are escaped the same way stdout is.
    let encoded_canonical = serde_json::to_string(&canonical).map_err(|error| error.to_string())?;
    if String::from_utf8_lossy(&output.stdout).contains(&encoded_canonical) {
        return Err("MCP stdout leaked the canonical repository path".to_string());
    }
    if responses[5].get("result") != Some(&json!({})) {
        return Err("legacy ping did not return an empty result".to_string());
    }
    Ok(())
}

/// After `initialize`, `ping` is identified by method name. A client that
/// attaches the handshake `_meta` this server requires pre-init (#5267)
/// must still receive an empty result; empty or omitted params are not
/// sufficient proof (#6022).
#[test]
fn post_initialize_ping_with_handshake_meta_returns_an_empty_result() -> Result<(), String> {
    let root = workspace_root()?;
    let request_bytes = [
        line(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "ripr-integration-test", "version": "1" }
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "ping-meta",
            "method": "ping",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "ping-empty",
            "method": "ping",
            "params": {}
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "ping-omitted",
            "method": "ping"
        }))?,
    ]
    .concat();
    let output = run_mcp(&root, &[&request_bytes])?;
    let responses = response_lines(&output)?;
    if responses.len() != 4 {
        return Err(format!(
            "expected initialize plus three ping replies, got {}: {responses:?}",
            responses.len()
        ));
    }
    if responses[0]
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
        != Some("2025-11-25")
    {
        return Err("initialize did not negotiate 2025-11-25".to_string());
    }
    for (index, id) in [
        (1usize, "ping-meta"),
        (2, "ping-empty"),
        (3, "ping-omitted"),
    ] {
        let response = responses
            .get(index)
            .ok_or_else(|| format!("missing ping reply for {id}"))?;
        if response.get("id") != Some(&json!(id)) {
            return Err(format!("ping reply lost its request id: {response}"));
        }
        if response.get("result") != Some(&json!({})) {
            return Err(format!(
                "post-initialize ping {id} must return an empty result, not {response}"
            ));
        }
    }
    Ok(())
}

#[test]
fn rejection_arms_survive_the_stdio_transport() -> Result<(), String> {
    let root = workspace_root()?;
    let request_bytes = [
        line(json!({
            "jsonrpc": "2.0",
            "id": "discover",
            "method": "server/discover",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "unknown-tool",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_everything",
                "arguments": {}
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "with-arguments",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_workspace_status",
                "arguments": { "verbose": true }
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "unknown-resource",
            "method": "resources/read",
            "params": {
                "_meta": current_meta(),
                "uri": "ripr://workspace/missing"
            }
        }))?,
    ]
    .concat();
    let output = run_mcp(&root, &[&request_bytes])?;
    let responses = response_lines(&output)?;
    if responses.len() != 4 {
        return Err(format!("expected 4 MCP responses, got {}", responses.len()));
    }
    let expected: [(&str, i64); 3] = [
        ("unknown-tool", -32601),
        ("with-arguments", -32602),
        ("unknown-resource", -32602),
    ];
    for (index, (id, code)) in expected.iter().enumerate() {
        let response = &responses[index + 1];
        if response.pointer("/error/code").and_then(Value::as_i64) != Some(*code) {
            return Err(format!(
                "rejection for {id} must retain error code {code}: {response}"
            ));
        }
        if response.pointer("/id").and_then(Value::as_str) != Some(id) {
            return Err(format!(
                "rejection for {id} must echo the request id: {response}"
            ));
        }
    }
    Ok(())
}

#[test]
fn current_discovery_requires_metadata_and_rejects_legacy_ping() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("ripr-mcp-missing-root-{}", std::process::id()));
    if root.is_dir() {
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove stale test root directory: {error}"))?;
    } else if root.exists() {
        std::fs::remove_file(&root)
            .map_err(|error| format!("remove stale test root file: {error}"))?;
    }
    let request_bytes = [
        line(json!({
            "jsonrpc": "2.0",
            "id": "discover",
            "method": "server/discover",
            "params": { "_meta": current_meta() }
        }))?,
        // The rejection row proves discovery cannot pass without the
        // required `_meta` metadata (negative experiment for the gate).
        line(json!({
            "jsonrpc": "2.0",
            "id": "discover-no-meta",
            "method": "server/discover",
            "params": {}
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "method": "notifications/cancelled",
            "params": { "requestId": "unused", "reason": "test" }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "tools",
            "method": "tools/list",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "status",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_workspace_status",
                "arguments": {}
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "ping",
            "method": "ping",
            "params": { "_meta": current_meta() }
        }))?,
    ]
    .concat();
    let output = run_mcp(&root, &[&request_bytes])?;
    let responses = response_lines(&output)?;
    if responses.len() != 5 {
        return Err(format!(
            "cancellation notification must not emit a response; got {} lines",
            responses.len()
        ));
    }
    if responses[0]
        .pointer("/result/supportedVersions")
        .and_then(Value::as_array)
        .is_none_or(|versions| !versions.iter().any(|value| value == "2026-07-28"))
    {
        return Err("discovery omitted the current protocol version".to_string());
    }
    // The missing-metadata discovery must be rejected by protocol error,
    // never accepted (negative experiment for the _meta gate).
    if responses[1].pointer("/error/code").and_then(Value::as_i64) != Some(-32602) {
        return Err(format!(
            "discovery without _meta must be invalid-params: {}",
            responses[1]
        ));
    }
    if responses[1].pointer("/id").and_then(Value::as_str) != Some("discover-no-meta") {
        return Err(format!(
            "discovery without _meta must retain its request id: {}",
            responses[1]
        ));
    }
    if responses[2]
        .pointer("/result/resultType")
        .and_then(Value::as_str)
        != Some("complete")
    {
        return Err("current tools/list result omitted resultType".to_string());
    }
    if responses[3]
        .pointer("/result/structuredContent/workspace/workspace_state")
        .and_then(Value::as_str)
        != Some("unavailable")
        || responses[3]
            .pointer("/result/structuredContent/workspace/root/error_code")
            .and_then(Value::as_str)
            != Some("root_missing")
    {
        return Err("invalid explicit root did not fail closed in status".to_string());
    }
    // The text a host shows the model names the cause and the recovery.
    let recovery = responses[3]
        .pointer("/result/content/1/text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !recovery.contains("Workspace unavailable (root_missing)")
        || !recovery.contains("ripr mcp --stdio --root <repository>")
    {
        return Err(format!(
            "unavailable status must carry a recovery: {}",
            responses[3]
        ));
    }
    let rejected_root = root.to_string_lossy();
    if String::from_utf8_lossy(&output.stdout).contains(rejected_root.as_ref()) {
        return Err("invalid explicit root leaked into MCP stdout".to_string());
    }
    if responses[4].pointer("/error/code").and_then(Value::as_i64) != Some(-32601) {
        return Err("current lifecycle ping must be method-not-found".to_string());
    }
    Ok(())
}

#[test]
fn unsupported_inline_version_is_rejected_and_next_request_recovers() -> Result<(), String> {
    let mut unsupported = current_meta();
    unsupported
        .as_object_mut()
        .ok_or_else(|| "fixture metadata must be an object".to_string())?
        .insert(
            "io.modelcontextprotocol/protocolVersion".into(),
            json!("2099-01-01"),
        );
    let requests = [
        line(
            json!({"jsonrpc":"2.0","id":"discover","method":"server/discover",
            "params":{"_meta":current_meta()}}),
        )?,
        line(
            json!({"jsonrpc":"2.0","id":"unsupported","method":"tools/list",
            "params":{"_meta":unsupported}}),
        )?,
        line(
            json!({"jsonrpc":"2.0","id":"recovered","method":"tools/list",
            "params":{"_meta":current_meta()}}),
        )?,
    ]
    .concat();
    let output = run_mcp(&workspace_root()?, &[&requests])?;
    let responses = response_lines(&output)?;
    if responses.len() != 3 {
        return Err("inline version control did not produce three correlated replies".into());
    }
    let refused = responses
        .get(1)
        .ok_or_else(|| "version refusal missing".to_string())?;
    let expected = serde_json::to_value(rmcp::model::ErrorCode::UNSUPPORTED_PROTOCOL_VERSION)
        .map_err(|error| error.to_string())?;
    if refused.get("id") != Some(&json!("unsupported"))
        || refused.pointer("/error/code") != Some(&expected)
    {
        return Err(
            "unsupported inline version was not rejected with its correlated SDK code".into(),
        );
    }
    let recovered = responses
        .get(2)
        .ok_or_else(|| "recovered response missing".to_string())?;
    if recovered.get("id") != Some(&json!("recovered"))
        || recovered
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .is_none_or(|tools| tools.len() != 8)
    {
        return Err("valid inline request did not recover the eight-tool slice surface".into());
    }
    Ok(())
}

/// The evidence tools fail closed on the wire before the first refresh:
/// a stock-shaped raw client gets typed structured failures (never a
/// partial document), and the snapshot/gap resource reads route through the
/// same typed state. No analysis runs in this control.
#[test]
fn gap_tools_fail_closed_before_the_first_refresh() -> Result<(), String> {
    let root = workspace_root()?;
    let request_bytes = [
        line(json!({
            "jsonrpc": "2.0",
            "id": "discover",
            "method": "server/discover",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "templates",
            "method": "resources/templates/list",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "list",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_list_gaps",
                "arguments": {}
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "get",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_get_gap",
                "arguments": { "canonical_id": "gap:any" }
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "prepare",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_prepare_repair",
                "arguments": { "canonical_id": "gap:any" }
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "attempt",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_get_repair_attempt",
                "arguments": { "attempt_id": "repair-attempt:absent" }
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "receipt",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_get_receipt_status",
                "arguments": { "receipt_id": "receipt:absent" }
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "card",
            "method": "tools/call",
            "params": {
                "_meta": current_meta(),
                "name": "ripr_get_repair_card",
                "arguments": { "canonical_id": "gap:any" }
            }
        }))?,
        line(json!({
            "jsonrpc": "2.0",
            "id": "snapshot",
            "method": "resources/read",
            "params": {
                "_meta": current_meta(),
                "uri": "ripr://snapshot/snapshot:sha256:absent"
            }
        }))?,
    ]
    .concat();
    let output = run_mcp(&root, &[&request_bytes])?;
    let responses = response_lines(&output)?;
    if responses.len() != 9 {
        return Err(format!("expected 9 MCP responses, got {}", responses.len()));
    }
    // rmcp answers concurrently, so key every response by request id instead
    // of assuming positional order.
    let by_id: std::collections::HashMap<&str, &Value> = responses
        .iter()
        .filter_map(|response| {
            response
                .pointer("/id")
                .and_then(Value::as_str)
                .map(|id| (id, response))
        })
        .collect();
    let templates = by_id
        .get("templates")
        .and_then(|response| response.pointer("/result/resourceTemplates"))
        .and_then(Value::as_array)
        .ok_or_else(|| "resources/templates/list omitted resourceTemplates".to_string())?;
    let template_uris = templates
        .iter()
        .filter_map(|template| template.pointer("/uriTemplate").and_then(Value::as_str))
        .collect::<Vec<_>>();
    for expected in [
        "ripr://snapshot/{snapshot_id}",
        "ripr://gap/{canonical_id}",
        "ripr://repair-attempt/{attempt_id}",
        "ripr://receipt/{receipt_id}",
        "ripr://repair-card/{canonical_id}",
    ] {
        if !template_uris.contains(&expected) {
            return Err(format!(
                "resource templates lost {expected}: {template_uris:?}"
            ));
        }
    }
    for id in ["list", "get", "prepare", "card"] {
        let response = by_id
            .get(id)
            .ok_or_else(|| format!("{id} response missing: {responses:?}"))?;
        if response.pointer("/result/isError").and_then(Value::as_bool) != Some(true) {
            return Err(format!(
                "{id} must fail closed with isError before refresh: {response}"
            ));
        }
        if response
            .pointer("/result/structuredContent/failure/code")
            .and_then(Value::as_str)
            != Some("no_snapshot")
        {
            return Err(format!(
                "{id} must fail closed with typed no_snapshot: {response}"
            ));
        }
    }
    // The repair-attempt and receipt lookups route through the durable
    // read-only store, so before any refresh they fail closed with the typed
    // not-found code instead of no_snapshot.
    for id in ["attempt", "receipt"] {
        let response = by_id
            .get(id)
            .ok_or_else(|| format!("{id} response missing: {responses:?}"))?;
        if response.pointer("/result/isError").and_then(Value::as_bool) != Some(true) {
            return Err(format!(
                "{id} must fail closed with isError before refresh: {response}"
            ));
        }
        if response
            .pointer("/result/structuredContent/failure/code")
            .and_then(Value::as_str)
            != Some("attempt_not_found")
        {
            return Err(format!(
                "{id} must fail closed with typed attempt_not_found: {response}"
            ));
        }
    }
    let snapshot = by_id
        .get("snapshot")
        .ok_or_else(|| format!("snapshot response missing: {responses:?}"))?;
    if snapshot.pointer("/error/code").and_then(Value::as_i64) != Some(-32602) {
        return Err(format!(
            "unknown snapshot resource must stay a current-version resource miss: {snapshot}"
        ));
    }
    if snapshot.pointer("/error/data/code").and_then(Value::as_str) != Some("no_snapshot") {
        return Err(format!(
            "unknown snapshot resource lost its typed state: {snapshot}"
        ));
    }
    Ok(())
}

/// Refresh on a small fixture can outlive the scripted `run_mcp` 10s
/// deadline; this session writes one request only after the previous
/// matching reply, the same production-stdio custody as `mcp_workspace_config`.
struct SequentialStdio {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    replies: std::sync::mpsc::Receiver<Value>,
    stdout: Option<std::thread::JoinHandle<Vec<u8>>>,
    stderr: Option<std::thread::JoinHandle<Vec<u8>>>,
}

impl SequentialStdio {
    fn spawn(root: &Path) -> Result<Self, String> {
        let mut child = worktree_ripr()
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
        })
    }

    fn call(
        &mut self,
        id: &str,
        method: &str,
        mut params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if let Some(object) = params.as_object_mut() {
            object.insert("_meta".to_string(), current_meta());
        }
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut encoded = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        encoded.push(b'\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "the session stdin was already released".to_string())?;
        stdin
            .write_all(&encoded)
            .map_err(|error| format!("write {method}: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("flush {method}: {error}"))?;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!(
                    "timed out waiting for the {method} reply id {id:?}"
                ));
            }
            let reply = self
                .replies
                .recv_timeout(remaining)
                .map_err(|_disconnected| {
                    format!("reply channel closed before the {method} reply id {id:?}")
                })?;
            if reply.get("id").and_then(Value::as_str) == Some(id) {
                return Ok(reply);
            }
        }
    }

    fn call_tool(
        &mut self,
        id: &str,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        self.call(
            id,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
            timeout,
        )
    }
}

impl Drop for SequentialStdio {
    fn drop(&mut self) {
        self.stdin = None;
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

struct FixtureGuard {
    root: PathBuf,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn commit_fixture(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=ripr fixture",
            "-c",
            "user.email=fixture@ripr.invalid",
            "commit",
            "-qm",
            message,
        ],
    )
}

/// B4 repair-ready fixture: main holds `>=`, the checked-out `journey`
/// branch narrows to `>`, and `discounted_total(100, 100) == 90` pins the
/// boundary. Same shape as `agentic_bench_mcp`'s B4 control.
fn install_b4_repair_ready_fixture() -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("clock before Unix epoch: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-mcp-stdio-repair-card-b4-{}-{stamp}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"mcp-journey-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\nname = \"mcp_journey_fixture\"\npath = \"src/lib.rs\"\n\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("tests/pricing.rs"),
        "use mcp_journey_fixture::discounted_total;\n\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n\n#[test]\nfn exact_boundary_gets_the_discount() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n",
    )
    .map_err(|error| error.to_string())?;
    fixture_git_ok(&root, &["-c", "init.defaultBranch=main", "init", "-q"])?;
    fixture_git_ok(&root, &["config", "user.name", "ripr fixture"])?;
    fixture_git_ok(&root, &["config", "user.email", "fixture@ripr.invalid"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    commit_fixture(&root, "open boundary")?;
    fixture_git_ok(&root, &["checkout", "-q", "-b", "journey"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount > discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
    )
    .map_err(|error| error.to_string())?;
    commit_fixture(&root, "closed boundary")?;
    Ok(root)
}

fn structured_tool_success(reply: &Value, context: &str) -> Result<Value, String> {
    let result = reply
        .get("result")
        .ok_or_else(|| format!("{context} lost its result: {reply}"))?;
    if result
        .pointer("/isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!(
            "{context} returned an error instead of a card: {result}"
        ));
    }
    result
        .get("structuredContent")
        .cloned()
        .ok_or_else(|| format!("{context} omitted structuredContent: {result}"))
}

/// Presentation-only command displays bind the portable `.` on MCP and the
/// selected checkout on the CLI (#3999). They never enter `repair_card_id`.
fn blank_presentation_displays(card: &mut Value) {
    if let Some(display) = card.pointer_mut("/next_action/display") {
        *display = json!("");
    }
    if let Some(display) = card.pointer_mut("/canonical_next_action/command/display") {
        *display = json!("");
    }
}

fn run_cli_agent_card(root: &Path, seam_id: &str) -> Result<Value, String> {
    let output = worktree_ripr()
        .args([
            "agent",
            "card",
            "--root",
            &root.display().to_string(),
            "--seam-id",
            seam_id,
            "--json",
        ])
        .output()
        .map_err(|error| format!("spawn ripr agent card: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ripr agent card failed for seam {seam_id}: status {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "ripr agent card stdout is not JSON: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// After refresh on the B4 repair-ready fixture, `ripr_get_repair_card`
/// must ship `ripr-mcp-repair-card-v1` whose inner card matches
/// `ripr agent card` for the same seam. A wrong schema, an error instead
/// of a card, or a diverging card fails this control (#7144).
#[test]
fn get_repair_card_after_refresh_matches_cli_agent_card() -> Result<(), String> {
    const REFRESH_TIMEOUT: Duration = Duration::from_mins(3);
    const REPLY_TIMEOUT: Duration = Duration::from_mins(1);
    let root = install_b4_repair_ready_fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let mut session = SequentialStdio::spawn(&root)?;
    session.call("discover", "server/discover", json!({}), REPLY_TIMEOUT)?;

    let refresh = session.call_tool("refresh", "ripr_refresh", json!({}), REFRESH_TIMEOUT)?;
    let refresh = structured_tool_success(&refresh, "ripr_refresh")?;
    if refresh
        .pointer("/snapshot/finding_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        == 0
    {
        return Err(format!(
            "B4 refresh must yield a gap before the card success path can run: {refresh}"
        ));
    }

    let list = session.call_tool("list", "ripr_list_gaps", json!({}), REPLY_TIMEOUT)?;
    let list = structured_tool_success(&list, "ripr_list_gaps")?;
    let items = list
        .pointer("/items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("ripr_list_gaps omitted items: {list}"))?;
    let canonical_id = items
        .iter()
        .find(|item| {
            item.pointer("/file")
                .and_then(Value::as_str)
                .is_some_and(|file| file.ends_with("src/lib.rs"))
        })
        .and_then(|item| item.pointer("/canonical_id").and_then(Value::as_str))
        .ok_or_else(|| format!("B4 list_gaps has no src/lib.rs item: {list}"))?
        .to_string();

    let card_reply = session.call_tool(
        "card",
        "ripr_get_repair_card",
        json!({ "canonical_id": canonical_id }),
        REPLY_TIMEOUT,
    )?;
    drop(session);
    let document = structured_tool_success(&card_reply, "ripr_get_repair_card")?;
    if document.pointer("/schema_version").and_then(Value::as_str)
        != Some("ripr-mcp-repair-card-v1")
    {
        return Err(format!(
            "repair-card success path lost schema ripr-mcp-repair-card-v1: {document}"
        ));
    }
    let mcp_card = document
        .get("card")
        .cloned()
        .ok_or_else(|| format!("repair-card document omitted card: {document}"))?;
    let repair_card_id = mcp_card
        .pointer("/repair_card_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("repair-card success path omitted repair_card_id: {mcp_card}"))?;
    let seam_id = mcp_card
        .pointer("/subject/seam_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("repair-card success path omitted subject.seam_id: {mcp_card}"))?;

    let cli_card = run_cli_agent_card(&root, seam_id)?;
    if cli_card.pointer("/schema_version").and_then(Value::as_str) != Some("repair_card.v1") {
        return Err(format!("ripr agent card lost repair_card.v1: {cli_card}"));
    }
    if cli_card.pointer("/repair_card_id").and_then(Value::as_str) != Some(repair_card_id) {
        return Err(format!(
            "MCP card diverged from `ripr agent card` identity: mcp={repair_card_id} cli={}",
            cli_card.pointer("/repair_card_id").unwrap_or(&Value::Null)
        ));
    }
    let mut mcp_compared = mcp_card;
    let mut cli_compared = cli_card;
    blank_presentation_displays(&mut mcp_compared);
    blank_presentation_displays(&mut cli_compared);
    if mcp_compared != cli_compared {
        return Err(format!(
            "MCP card diverged from `ripr agent card` after blanking presentation displays:\nmcp={mcp_compared}\ncli={cli_compared}"
        ));
    }
    Ok(())
}
