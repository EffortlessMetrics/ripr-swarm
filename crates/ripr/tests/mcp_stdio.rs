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

#[cfg(feature = "lang-rust")]
#[path = "support/mcp_scope_contract.rs"]
mod scope_contract;
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

/// Optional tool fields may be omitted, but their advertised string/integer
/// types do not accept explicit null. Pin the schemas and dispatch separately
/// against literal expectations, through the actual stdio process (#7314).
#[test]
fn optional_tool_arguments_reject_null_and_recover_over_stdio() -> Result<(), String> {
    let root = workspace_root()?;
    let cases = [
        (
            "ripr_list_gaps",
            "snapshot_id",
            json!({}),
            json!("snapshot:sha256:input-control"),
            "string",
        ),
        ("ripr_list_gaps", "offset", json!({}), json!(0), "integer"),
        ("ripr_list_gaps", "limit", json!({}), json!(1), "integer"),
        (
            "ripr_list_gaps",
            "file",
            json!({}),
            json!("src/input-control.rs"),
            "string",
        ),
        (
            "ripr_list_gaps",
            "class",
            json!({}),
            json!("weakly_exposed"),
            "string",
        ),
        (
            "ripr_list_gaps",
            "language",
            json!({}),
            json!("rust"),
            "string",
        ),
        (
            "ripr_get_gap",
            "snapshot_id",
            json!({"canonical_id": "gap:input-control"}),
            json!("snapshot:sha256:input-control"),
            "string",
        ),
        (
            "ripr_prepare_repair",
            "snapshot_id",
            json!({"canonical_id": "gap:input-control"}),
            json!("snapshot:sha256:input-control"),
            "string",
        ),
        (
            "ripr_get_repair_card",
            "snapshot_id",
            json!({"canonical_id": "gap:input-control"}),
            json!("snapshot:sha256:input-control"),
            "string",
        ),
    ];
    let tool_call = |id: &str, name: &str, arguments: Value| {
        line(json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "_meta": current_meta(), "name": name, "arguments": arguments }
        }))
    };
    let mut requests = vec![
        line(json!({
            "jsonrpc": "2.0", "id": "discover", "method": "server/discover",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0", "id": "tools", "method": "tools/list",
            "params": { "_meta": current_meta() }
        }))?,
    ];
    for (index, (tool, field, omitted, valid, _kind)) in cases.iter().enumerate() {
        for (variant, value) in [
            ("omitted", None),
            ("valid", Some(valid.clone())),
            ("null", Some(Value::Null)),
        ] {
            let mut arguments = omitted.clone();
            if let Some(value) = value {
                arguments
                    .as_object_mut()
                    .ok_or("control arguments must be an object")?
                    .insert((*field).into(), value);
            }
            requests.push(tool_call(&format!("{index}-{variant}"), tool, arguments)?);
        }
    }
    requests.push(tool_call(
        "wrong-type",
        "ripr_list_gaps",
        json!({"limit": false}),
    )?);
    requests.push(tool_call("recovery", "ripr_workspace_status", json!({}))?);
    let input = requests.concat();
    let output = run_mcp(&root, &[&input])?;
    let responses = response_lines(&output)?;
    if responses.len() != requests.len() {
        return Err(format!(
            "expected {} correlated replies, got {}",
            requests.len(),
            responses.len()
        ));
    }
    if output
        .stdout
        .split(|byte| *byte == b'\n')
        .any(|frame| frame.len() > 128 * 1024)
    {
        return Err("argument control exceeded the literal 128-KiB response bound".into());
    }
    let reply = |id: &str| {
        responses
            .iter()
            .find(|response| response.get("id").and_then(Value::as_str) == Some(id))
            .ok_or_else(|| format!("missing correlated response for {id}"))
    };
    let discovered = reply("discover")?;
    if discovered.get("error").is_some()
        || !discovered
            .pointer("/result/supportedVersions")
            .and_then(Value::as_array)
            .is_some_and(|versions| {
                versions
                    .iter()
                    .any(|version| version.as_str() == Some("2026-07-28"))
            })
    {
        return Err("control did not reach real server discovery".into());
    }
    let tools = reply("tools")?
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .ok_or("discovery omitted tools")?;
    for (index, (tool, field, _omitted, _valid, kind)) in cases.iter().enumerate() {
        let descriptor = tools
            .iter()
            .find(|descriptor| descriptor.get("name").and_then(Value::as_str) == Some(*tool))
            .ok_or_else(|| format!("discovery omitted {tool}"))?;
        let schema = descriptor
            .get("inputSchema")
            .ok_or("tool omitted input schema")?;
        if schema
            .pointer(&format!("/properties/{field}/type"))
            .and_then(Value::as_str)
            != Some(*kind)
            || schema
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|fields| fields.iter().any(|value| value.as_str() == Some(*field)))
        {
            return Err(format!(
                "{tool}.{field} must stay optional with non-null {kind} type: {schema}"
            ));
        }
        for variant in ["omitted", "valid"] {
            let accepted = reply(&format!("{index}-{variant}"))?;
            if accepted.get("error").is_some()
                || accepted.pointer("/result/isError").and_then(Value::as_bool) != Some(true)
                || accepted
                    .pointer("/result/structuredContent/failure/code")
                    .and_then(Value::as_str)
                    != Some("no_snapshot")
            {
                return Err(format!(
                    "{tool}.{field} {variant} must reach session lookup: {accepted}"
                ));
            }
        }
        let rejected = reply(&format!("{index}-null"))?;
        let expected_message = format!(
            "{field} must be a {}",
            if *kind == "integer" {
                "non-negative integer"
            } else {
                "string"
            }
        );
        if rejected.pointer("/error/code").and_then(Value::as_i64) != Some(-32602)
            || rejected.pointer("/error/message").and_then(Value::as_str)
                != Some(expected_message.as_str())
            || rejected.get("result").is_some()
        {
            return Err(format!(
                "{tool}.{field} null must be correlated Invalid Params before session lookup: {rejected}"
            ));
        }
    }
    if reply("wrong-type")?
        .pointer("/error/code")
        .and_then(Value::as_i64)
        != Some(-32602)
    {
        return Err("boolean paging limit must remain Invalid Params".into());
    }
    let recovered = reply("recovery")?;
    if recovered.get("error").is_some()
        || recovered
            .pointer("/result/isError")
            .and_then(Value::as_bool)
            != Some(false)
        || recovered
            .pointer("/result/structuredContent/schema_version")
            .and_then(Value::as_str)
            != Some("ripr-mcp-workspace-status-v1")
        || recovered
            .pointer("/result/structuredContent/session/attempt_state")
            .and_then(Value::as_str)
            != Some("no_snapshot")
    {
        return Err(format!(
            "status must succeed unchanged after argument errors: {recovered}"
        ));
    }
    Ok(())
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
fn pre_call_contract_discloses_committed_scope_on_wire_and_help() -> Result<(), String> {
    let root = workspace_root()?;
    let requests = [
        line(json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": { "name": "scope-contract-test", "version": "1" } }
        }))?,
        line(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))?,
        line(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }))?,
    ]
    .concat();
    let output = run_mcp(&root, &[&requests])?;
    let replies = response_lines(&output)?;
    if replies.len() != 2 {
        return Err(format!(
            "expected initialize and tools/list replies: {replies:?}"
        ));
    }
    let instructions = replies[0]
        .pointer("/result/instructions")
        .and_then(Value::as_str)
        .ok_or_else(|| "initialize omitted instructions".to_string())?;
    let refresh = replies[1]
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .and_then(|tools| tools.iter().find(|tool| tool["name"] == "ripr_refresh"))
        .ok_or_else(|| "tools/list omitted ripr_refresh".to_string())?;
    let description = refresh
        .get("description")
        .and_then(Value::as_str)
        .ok_or_else(|| "ripr_refresh omitted its description".to_string())?;
    if refresh.pointer("/inputSchema/properties") != Some(&json!({})) {
        return Err(format!(
            "refresh must retain its argument-free scope: {refresh}"
        ));
    }
    let discovery = line(json!({ "jsonrpc": "2.0", "id": 1,
        "method": "server/discover", "params": { "_meta": current_meta() } }))?;
    let discover_output = run_mcp(&root, &[&discovery])?;
    let discovered = response_lines(&discover_output)?;
    let discover_instructions = discovered
        .first()
        .and_then(|reply| reply.pointer("/result/instructions"))
        .and_then(Value::as_str)
        .ok_or_else(|| "server/discover omitted instructions".to_string())?;
    let help = worktree_ripr()
        .args(["mcp", "--help"])
        .output()
        .map_err(|error| format!("run MCP help: {error}"))?;
    if !help.status.success() {
        return Err(format!("MCP help failed: {:?}", help.status.code()));
    }
    let help = String::from_utf8(help.stdout).map_err(|error| error.to_string())?;
    // Literal acceptance expectations, independent of descriptor constants.
    // Test the delivered surfaces before any analysis call, even on a clean root.
    let mut failures = Vec::new();
    for (surface, text) in [
        ("initialize", instructions),
        ("tools/list ripr_refresh", description),
        ("server/discover", discover_instructions),
        ("mcp --help", help.as_str()),
    ] {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for required in [
            "committed default-branch diff",
            "staged and unstaged tracked edits and untracked files are outside",
            "`ripr check` defaults to the working tree when staged or unstaged tracked edits exist",
            "shared check authority does not mean identical input",
            "`ripr check --committed`",
        ] {
            if !text.contains(required) {
                failures.push(format!("{surface} omitted {required:?}"));
            }
        }
        if text.contains("the same analysis `ripr check`") {
            failures.push(format!("{surface} promises identical CLI input"));
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("\n"));
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
fn malformed_params_of_a_served_method_answer_invalid_params_and_the_session_recovers()
-> Result<(), String> {
    let root = workspace_root()?;
    let request_bytes = [
        line(json!({
            "jsonrpc": "2.0", "id": "discover", "method": "server/discover",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0", "id": "string-arguments", "method": "tools/call",
            "params": { "_meta": current_meta(), "name": "ripr_workspace_status", "arguments": "oops" }
        }))?,
        line(json!({
            "jsonrpc": "2.0", "id": "no-name", "method": "tools/call",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0", "id": "unserved", "method": "nope/never",
            "params": { "_meta": current_meta() }
        }))?,
        line(json!({
            "jsonrpc": "2.0", "id": "recovered-status", "method": "resources/read",
            "params": { "_meta": current_meta(), "uri": "ripr://workspace/status" }
        }))?,
    ]
    .concat();
    let output = run_mcp(&root, &[&request_bytes])?;
    let responses = response_lines(&output)?;
    let expected: [(&str, i64); 3] = [
        ("string-arguments", -32602),
        ("no-name", -32602),
        ("unserved", -32601),
    ];
    if responses.len() != 5 {
        return Err(format!("expected 5 MCP responses, got {}", responses.len()));
    }
    for (index, (id, code)) in expected.iter().enumerate() {
        let response = &responses[index + 1];
        if response.pointer("/id").and_then(Value::as_str) != Some(id) {
            return Err(format!(
                "rejection for {id} must echo the request id: {response}"
            ));
        }
        if response.pointer("/error/code").and_then(Value::as_i64) != Some(*code) {
            return Err(format!(
                "rejection for {id} must use code {code}: {response}"
            ));
        }
    }
    if !responses[1]
        .pointer("/error/message")
        .and_then(Value::as_str)
        .is_some_and(|message| message.contains("tools/call"))
    {
        return Err(format!(
            "invalid-params message must name the method: {}",
            responses[1]
        ));
    }
    if responses[4].get("result").is_none() {
        return Err(format!(
            "session must recover after rejections: {}",
            responses[4]
        ));
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
        line(json!({
            "jsonrpc": "2.0", "id": "missing-snapshot", "method": "resources/read",
            "params": { "_meta": current_meta(), "uri": "ripr://snapshot/snapshot:sha256:absent" }
        }))?,
        line(json!({
            "jsonrpc": "2.0", "id": "recovered-status", "method": "resources/read",
            "params": { "_meta": current_meta(), "uri": "ripr://workspace/status" }
        }))?,
    ]
    .concat();
    let output = run_mcp(&root, &[&request_bytes])?;
    let responses = response_lines(&output)?;
    if responses.len() != 6 {
        return Err(format!("expected 6 MCP responses, got {}", responses.len()));
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
    let by_id = responses
        .iter()
        .map(|response| {
            response
                .get("id")
                .and_then(Value::as_str)
                .map(|id| (id, response))
                .ok_or_else(|| format!("response lost its string id: {response}"))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
    if by_id.len() != 6 {
        return Err("resource recovery responses must have distinct ids".to_string());
    }
    let discover = by_id.get("discover").ok_or("missing discovery response")?;
    if !discover.get("result").is_some_and(Value::is_object) || discover.get("error").is_some() {
        return Err(format!(
            "discovery failed before recovery controls: {discover}"
        ));
    }
    let snapshot = by_id
        .get("missing-snapshot")
        .ok_or("missing snapshot response")?;
    if snapshot.pointer("/error/code").and_then(Value::as_i64) != Some(-32602)
        || snapshot.pointer("/error/message").and_then(Value::as_str)
            != Some("unavailable snapshot resource: no_snapshot")
        || snapshot.pointer("/error/data/code").and_then(Value::as_str) != Some("no_snapshot")
        || snapshot.pointer("/error/data/available").is_some()
        || snapshot.pointer("/error/data/resource_templates").is_some()
    {
        return Err(format!(
            "recognized snapshot lost its typed refusal: {snapshot}"
        ));
    }
    let status = by_id
        .get("recovered-status")
        .ok_or("missing status recovery response")?;
    if status.get("error").is_some()
        || status
            .pointer("/result/contents/0/uri")
            .and_then(Value::as_str)
            != Some("ripr://workspace/status")
    {
        return Err(format!(
            "valid status read failed after resource miss: {status}"
        ));
    }
    let status_text = status
        .pointer("/result/contents/0/text")
        .and_then(Value::as_str)
        .ok_or("recovered status omitted JSON document")?;
    let status_document: Value =
        serde_json::from_str(status_text).map_err(|error| error.to_string())?;
    if status_document
        .pointer("/workspace/authority/source_edit_capability")
        .and_then(Value::as_str)
        != Some("none")
        || status_document
            .get("schema_version")
            .and_then(Value::as_str)
            != Some("ripr-mcp-workspace-status-v1")
        || status_document
            .pointer("/session/attempt_state")
            .and_then(Value::as_str)
            != Some("no_snapshot")
    {
        return Err(format!(
            "recovered status lost its read-only contract: {status_document}"
        ));
    }
    // Literal consumer contract, independent of the production template registry.
    let miss = by_id
        .get("unknown-resource")
        .ok_or("missing unknown-resource response")?;
    let expected_message = "unknown resource; available: ripr://workspace/status; templates: ripr://snapshot/{snapshot_id}, ripr://gap/{canonical_id}, ripr://repair-attempt/{attempt_id}, ripr://receipt/{receipt_id}, ripr://repair-card/{canonical_id}";
    if miss.pointer("/error/data/available") != Some(&json!(["ripr://workspace/status"]))
        || miss.pointer("/error/data/resource_templates")
            != Some(&json!([
                "ripr://snapshot/{snapshot_id}",
                "ripr://gap/{canonical_id}",
                "ripr://repair-attempt/{attempt_id}",
                "ripr://receipt/{receipt_id}",
                "ripr://repair-card/{canonical_id}"
            ]))
    {
        return Err(format!(
            "resource miss changed structured recovery routes: {miss}"
        ));
    }
    if miss.pointer("/error/message").and_then(Value::as_str) != Some(expected_message) {
        return Err(format!(
            "unknown-resource message hides valid recovery routes: {miss}"
        ));
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
            .is_none_or(|tools| tools.len() != 9)
    {
        return Err("valid inline request did not recover the nine-tool slice surface".into());
    }
    Ok(())
}

/// The first evidence read must name the root problem and a recovery that
/// can change it, on tools and resources alike (#7346).
#[test]
fn unavailable_workspace_evidence_names_restart_before_any_refresh() -> Result<(), String> {
    struct FixtureRoot(PathBuf);
    impl Drop for FixtureRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let temp = std::env::temp_dir();
    std::fs::create_dir_all(&temp).map_err(|error| format!("create temp root: {error}"))?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let path = temp.join(format!(
        "ripr-mcp-unavailable-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&path).map_err(|error| format!("create markerless root: {error}"))?;
    let fixture = FixtureRoot(path);
    let missing = fixture.0.join("missing");
    for (root, root_error) in [
        (fixture.0.as_path(), "repository_marker_missing"),
        (missing.as_path(), "root_missing"),
    ] {
        let mut requests = vec![
            line(json!({
                "jsonrpc": "2.0", "id": "discover", "method": "server/discover",
                "params": { "_meta": current_meta() }
            }))?,
            line(json!({
                "jsonrpc": "2.0", "id": "status", "method": "tools/call",
                "params": { "_meta": current_meta(), "name": "ripr_workspace_status", "arguments": {} }
            }))?,
        ];
        for phase in ["before", "after"] {
            if phase == "after" {
                requests.push(line(json!({
                    "jsonrpc": "2.0", "id": "refresh", "method": "tools/call",
                    "params": { "_meta": current_meta(), "name": "ripr_refresh", "arguments": {} }
                }))?);
            }
            for (name, arguments) in [
                ("ripr_list_gaps", json!({})),
                ("ripr_get_gap", json!({ "canonical_id": "gap:any" })),
                ("ripr_prepare_repair", json!({ "canonical_id": "gap:any" })),
                ("ripr_get_repair_card", json!({ "canonical_id": "gap:any" })),
            ] {
                requests.push(line(json!({
                    "jsonrpc": "2.0", "id": format!("{phase}-{name}"), "method": "tools/call",
                    "params": { "_meta": current_meta(), "name": name, "arguments": arguments }
                }))?);
            }
            for uri in [
                "ripr://snapshot/snapshot:sha256:any",
                "ripr://gap/gap:any",
                "ripr://repair-card/gap:any",
            ] {
                requests.push(line(json!({
                    "jsonrpc": "2.0", "id": format!("{phase}-{uri}"), "method": "resources/read",
                    "params": { "_meta": current_meta(), "uri": uri }
                }))?);
            }
        }
        requests.push(line(json!({
            "jsonrpc": "2.0", "id": "survivor", "method": "tools/call",
            "params": { "_meta": current_meta(), "name": "ripr_workspace_status", "arguments": {} }
        }))?);
        let output = run_mcp(root, &[&requests.concat()])?;
        let responses = response_lines(&output)?;
        if responses.len() != 18 {
            return Err(format!(
                "expected 18 replies for {root_error}, got {}",
                responses.len()
            ));
        }
        let by_id: std::collections::HashMap<&str, &Value> = responses
            .iter()
            .filter_map(|response| {
                response
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| (id, response))
            })
            .collect();
        if by_id.len() != 18 {
            return Err(format!(
                "expected 18 unique reply ids for {root_error}, got {}",
                by_id.len()
            ));
        }
        for phase in ["before", "after"] {
            for subject in [
                "ripr_list_gaps",
                "ripr_get_gap",
                "ripr_prepare_repair",
                "ripr_get_repair_card",
                "ripr://snapshot/snapshot:sha256:any",
                "ripr://gap/gap:any",
                "ripr://repair-card/gap:any",
            ] {
                let id = format!("{phase}-{subject}");
                if !by_id.contains_key(id.as_str()) {
                    return Err(format!(
                        "missing intended evidence reply {id} for {root_error}"
                    ));
                }
            }
        }
        for id in ["status", "survivor"] {
            let reply = by_id.get(id).ok_or_else(|| format!("missing {id} reply"))?;
            if reply
                .pointer("/result/structuredContent/workspace/workspace_state")
                .and_then(Value::as_str)
                != Some("unavailable")
                || reply
                    .pointer("/result/structuredContent/workspace/root/error_code")
                    .and_then(Value::as_str)
                    != Some(root_error)
                || reply
                    .pointer("/result/structuredContent/session/attempt_state")
                    .and_then(Value::as_str)
                    != Some("no_snapshot")
            {
                return Err(format!(
                    "fixture or survivor state lost {root_error}: {reply}"
                ));
            }
        }
        let recovery = "restart the server with `ripr mcp --stdio --root <repository>`";
        let refresh = by_id
            .get("refresh")
            .ok_or_else(|| "missing refresh reply".to_string())?;
        let expected_failure = refresh
            .pointer("/result/structuredContent/failure")
            .ok_or_else(|| format!("refresh lost typed root failure: {refresh}"))?;
        if expected_failure.get("code").and_then(Value::as_str) != Some("workspace_unavailable")
            || expected_failure.get("recovery").and_then(Value::as_str) != Some(recovery)
        {
            return Err(format!("refresh lost the actionable recovery: {refresh}"));
        }
        for (id, reply) in &by_id {
            if !id.starts_with("before-") && !id.starts_with("after-") && *id != "refresh" {
                continue;
            }
            let resource = id.contains("ripr://");
            let failure = if resource {
                if reply.pointer("/error/code").and_then(Value::as_i64) != Some(-32602) {
                    return Err(format!("{id} lost resource failure semantics: {reply}"));
                }
                reply.pointer("/error/data/failure")
            } else {
                if reply.pointer("/result/isError").and_then(Value::as_bool) != Some(true) {
                    return Err(format!("{id} lost typed tool failure: {reply}"));
                }
                let text = reply
                    .pointer("/result/content/0/text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !text.contains("workspace_unavailable") || !text.contains(recovery) {
                    return Err(format!("{id} rendered an unusable recovery: {reply}"));
                }
                reply.pointer("/result/structuredContent/failure")
            };
            if failure
                .and_then(|value| value.get("code"))
                .and_then(Value::as_str)
                != Some("workspace_unavailable")
                || failure
                    .and_then(|value| value.get("recovery"))
                    .and_then(Value::as_str)
                    != Some(recovery)
                || failure != Some(expected_failure)
            {
                return Err(format!(
                    "{id} must share refresh's root failure before and after refresh: {reply}"
                ));
            }
        }
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

/// Real committed multi-item input for RIPR-SPEC-0211 / #7464. The two
/// pricing boundaries have strong test sites; the uncovered Rust predicates
/// and (when compiled) Python predicate provide distinct excluded subjects.
fn install_gap_filter_fixture() -> Result<PathBuf, String> {
    let root = install_b4_repair_ready_fixture()?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod pricing;\npub mod uncovered;\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join(".gitignore"), ".ripr/\ntarget/\n")
        .map_err(|error| error.to_string())?;
    let pricing = "pub fn discount(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold { amount - 10 } else { amount }\n}\npub fn fee(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold { amount + 5 } else { amount }\n}\n";
    let uncovered =
        "pub fn accepts(amount: i32, threshold: i32) -> bool {\n    amount >= threshold\n}\n";
    std::fs::write(root.join("src/pricing.rs"), pricing).map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/uncovered.rs"), uncovered).map_err(|error| error.to_string())?;
    std::fs::write(root.join("tests/pricing.rs"), "use mcp_journey_fixture::pricing::{discount, fee};\n#[test]\nfn discount_boundary() {\n    assert_eq!(discount(100, 100), 90);\n}\n#[test]\nfn fee_boundary() {\n    assert_eq!(fee(100, 100), 105);\n}\n").map_err(|error| error.to_string())?;
    let languages = if cfg!(feature = "lang-python") {
        std::fs::write(
            root.join("pyproject.toml"),
            "[project]\nname = \"mcp-filter-fixture\"\nversion = \"0.0.0\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/cart.py"), "def accepts(amount, threshold):\n    if amount > threshold:\n        return amount - 1\n    return amount\n").map_err(|error| error.to_string())?;
        "[languages]\nenabled = [\"rust\", \"python\"]\n"
    } else {
        "[languages]\nenabled = [\"rust\"]\n"
    };
    std::fs::write(root.join("ripr.toml"), languages).map_err(|error| error.to_string())?;
    commit_fixture(&root, "filter baseline")?;
    fixture_git_ok(&root, &["update-ref", "refs/heads/main", "HEAD"])?;
    std::fs::write(root.join("src/pricing.rs"), pricing.replace(">=", ">"))
        .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/uncovered.rs"), uncovered.replace(">=", ">"))
        .map_err(|error| error.to_string())?;
    if cfg!(feature = "lang-python") {
        std::fs::write(root.join("src/cart.py"), "def accepts(amount, threshold):\n    if amount >= threshold:\n        return amount - 1\n    return amount\n").map_err(|error| error.to_string())?;
    }
    commit_fixture(&root, "narrow Rust and widen Python boundaries")?;
    Ok(root)
}

fn check_filtered_gap_page(
    document: &Value,
    expected_ids: &[&str],
    selected: usize,
    filtered: usize,
    snapshot_id: &str,
) -> Result<(), String> {
    let items = document["items"].as_array().ok_or("list omitted items")?;
    let ids = items
        .iter()
        .map(|item| item["canonical_id"].as_str().ok_or("item omitted ID"))
        .collect::<Result<Vec<_>, _>>()?;
    if ids != expected_ids
        || document["selected"] != json!(selected)
        || document["filtered"] != json!(filtered)
        || document["page"]["returned"] != json!(expected_ids.len())
        || document["snapshot_id"] != snapshot_id
        || document["schema_version"] != "ripr-mcp-gap-list-v2"
    {
        return Err(format!(
            "filtered transport page differs from exact IDs/counts: expected {expected_ids:?}, selected={selected}, filtered={filtered}: {document}"
        ));
    }
    Ok(())
}

/// #7768: an empty page still carries its echoed filters. Exercise the
/// interval where that document fits but its required structured envelope
/// does not, using the real CLI transport and an independently sized result.
#[test]
fn empty_gap_pages_keep_structured_outcomes_at_the_response_bound() -> Result<(), String> {
    const RESPONSE_BOUND: usize = 128 * 1024;
    let timeout = Duration::from_secs(30);
    let root = install_b4_repair_ready_fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let (binary, digest) = Observation::executable_custody(Path::new(env!("CARGO_BIN_EXE_ripr")))?;
    eprintln!(
        "empty-page control binary={} sha256={digest}",
        binary.display()
    );
    let mut session = SequentialStdio::spawn(&root)?;
    let discovery = session.call("empty-discover", "server/discover", json!({}), timeout)?;
    if discovery.get("result").is_none() {
        return Err("empty-page fixture failed discovery".into());
    }
    let refresh = session.call_tool("empty-refresh", "ripr_refresh", json!({}), timeout)?;
    let refresh = structured_tool_success(&refresh, "empty-page refresh")?;
    if refresh["attempt"]["state"] != "completed"
        || refresh["snapshot"]["outcome_kind"] != "complete_with_findings"
    {
        return Err(format!(
            "empty-page fixture did not complete analysis: {refresh}"
        ));
    }
    let all = session.call_tool("empty-baseline", "ripr_list_gaps", json!({}), timeout)?;
    let all = structured_tool_success(&all, "empty-page baseline")?;
    let items = all["items"].as_array().ok_or("baseline omitted items")?;
    if items.is_empty() || all["selected"] != json!(items.len()) {
        return Err("empty-page fixture must first expose a nonempty selected set".into());
    }
    let snapshot_id = all["snapshot_id"]
        .as_str()
        .ok_or("baseline omitted snapshot ID")?;

    for field in ["file", "class", "language"] {
        let fitting_filter = "\u{1}".repeat(9_500);
        let mut arguments = json!({});
        arguments[field] = json!(fitting_filter);
        let fitting = session.call_tool("empty-fitting", "ripr_list_gaps", arguments, timeout)?;
        if serde_json::to_vec(&fitting)
            .map_err(|error| error.to_string())?
            .len()
            > RESPONSE_BOUND
        {
            return Err(format!(
                "fitting empty {field} page exceeded the response cap"
            ));
        }
        let fitting = structured_tool_success(&fitting, "fitting empty page")?;
        if fitting["items"] != json!([])
            || fitting["filtered"] != 0
            || fitting["selected"] != all["selected"]
            || fitting["filters"][field] != fitting_filter
            || fitting["snapshot_id"] != snapshot_id
            || fitting["page"]["has_more"] != false
            || fitting["page"]["next_offset"] != Value::Null
        {
            return Err(format!(
                "fitting empty {field} page lost its counts, echo, or identity"
            ));
        }

        // Size the consumer-required envelope independently of the pager's
        // fit helper. This stimulus must sit below the raw-document ceiling
        // and above the complete structured-envelope cap before testing refusal.
        let overflowing_filter = "\u{1}".repeat(10_000);
        let mut expected = fitting;
        expected["filters"][field] = json!(overflowing_filter);
        let text = serde_json::to_string(&expected).map_err(|error| error.to_string())?;
        let raw_bytes = text.len();
        let envelope = json!({
            "content": [{"type": "text", "text": text}],
            "structuredContent": expected,
            "isError": false,
        });
        let structured_bytes = serde_json::to_vec(&envelope)
            .map_err(|error| error.to_string())?
            .len();
        if raw_bytes > RESPONSE_BOUND / 2 - 256 || structured_bytes <= RESPONSE_BOUND {
            return Err(format!(
                "{field} overflow fixture missed its interval: raw={raw_bytes}, structured={structured_bytes}"
            ));
        }
        eprintln!(
            "empty {field} page raw={raw_bytes} structured={structured_bytes} bound={RESPONSE_BOUND}"
        );
        for filter in [overflowing_filter, "\u{1}".repeat(11_000)] {
            let mut arguments = json!({});
            arguments[field] = json!(filter);
            let refused =
                session.call_tool("empty-refused", "ripr_list_gaps", arguments, timeout)?;
            if refused.get("error").is_some()
                || refused.pointer("/result/isError") != Some(&json!(true))
                || refused.pointer("/result/structuredContent/schema_version")
                    != Some(&json!("ripr-mcp-gap-list-v2"))
                || refused.pointer("/result/structuredContent/failure/code")
                    != Some(&json!("result_too_large"))
                || refused.pointer("/result/structuredContent/failure/data/current_snapshot_id")
                    != Some(&json!(snapshot_id))
            {
                return Err(format!(
                    "empty {field} overflow must return typed result_too_large: isError={:?}, has_structured={}, code={:?}",
                    refused.pointer("/result/isError"),
                    refused.pointer("/result/structuredContent").is_some(),
                    refused.pointer("/result/structuredContent/failure/code")
                ));
            }
            let recovery = refused
                .pointer("/result/structuredContent/failure/recovery")
                .and_then(Value::as_str)
                .ok_or("empty-page refusal omitted recovery")?;
            if !recovery.contains("ripr://snapshot/{snapshot_id}")
                || !recovery.contains("ripr_get_gap")
                || serde_json::to_vec(&refused)
                    .map_err(|error| error.to_string())?
                    .len()
                    > RESPONSE_BOUND
            {
                return Err(format!(
                    "empty {field} refusal lost its bounded identity recovery"
                ));
            }
        }
    }
    let recovered = session.call_tool("empty-recovered", "ripr_list_gaps", json!({}), timeout)?;
    if structured_tool_success(&recovered, "same-session recovery")? != all {
        return Err("empty-page refusals changed the completed snapshot or selection".into());
    }
    Ok(())
}

/// #7768: the semantic response bound must account for the complete
/// serialized JSON-RPC response, not only the tool envelope. A legal,
/// escaping-heavy request id can push the complete response past the wire
/// cap while every envelope-only check still passes, which surfaced as the
/// correlated generic `-32603` fallback instead of the typed
/// `result_too_large` refusal. Both stimulus ids are computed here from
/// envelopes actually measured on this session's wire: one excludes every
/// `ripr_list_gaps` page from the id-aware envelope budget (typed refusal),
/// and one sits one notch below that so the identical call still ships a
/// structured page. Reverting the id-aware reservation fails the refusal
/// arm on the generic fallback; a reserve large enough to refuse fitting
/// pages fails the fit arm.
#[test]
fn long_request_ids_refuse_typed_before_the_wire_cap() -> Result<(), String> {
    const RESPONSE_BOUND: usize = 128 * 1024;
    const INPUT_CAP: usize = 256 * 1024;
    const OUTER_RESERVE: usize = 128;
    let timeout = Duration::from_secs(30);
    let root = install_b4_repair_ready_fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let mut session = SequentialStdio::spawn(&root)?;
    let discovery = session.call("outer-discover", "server/discover", json!({}), timeout)?;
    if discovery.get("result").is_none() {
        return Err("outer-budget fixture failed discovery".into());
    }
    let refresh = session.call_tool("outer-refresh", "ripr_refresh", json!({}), timeout)?;
    let refresh = structured_tool_success(&refresh, "outer-budget refresh")?;
    if refresh["attempt"]["state"] != "completed" {
        return Err(format!(
            "outer-budget fixture did not complete analysis: {refresh}"
        ));
    }

    // Measure both page shapes on the real wire with short ids. The
    // semantic envelope is the reply's result member minus the `resultType`
    // field the SDK may add after the semantic measurement; the fixed
    // per-response overhead absorbs whatever negotiated shape remains.
    let semantic_envelope_bytes = |reply: &Value, context: &str| -> Result<usize, String> {
        let mut result = reply
            .get("result")
            .cloned()
            .ok_or_else(|| format!("{context} lost its result: {reply}"))?;
        if let Some(object) = result.as_object_mut() {
            object.remove("resultType");
        }
        serde_json::to_vec(&result)
            .map(|bytes| bytes.len())
            .map_err(|error| error.to_string())
    };
    let frame_bytes = |reply: &Value, context: &str| -> Result<usize, String> {
        serde_json::to_vec(reply)
            .map(|bytes| bytes.len() + 1)
            .map_err(|error| format!("{context} could not be rendered: {error}"))
    };
    let empty_filter = "outer-budget-no-match";
    let empty_reply = session.call_tool(
        "outer-empty",
        "ripr_list_gaps",
        json!({ "file": empty_filter }),
        timeout,
    )?;
    let empty = structured_tool_success(&empty_reply, "outer-budget empty page")?;
    if empty["items"] != json!([]) || empty["filters"]["file"] != json!(empty_filter) {
        return Err(format!("empty-page measurement lost its shape: {empty}"));
    }
    let empty_bytes = semantic_envelope_bytes(&empty_reply, "empty page")?;
    let full_reply = session.call_tool("outer-full", "ripr_list_gaps", json!({}), timeout)?;
    let full = structured_tool_success(&full_reply, "outer-budget full page")?;
    let items = full["items"].as_array().ok_or("full page omitted items")?;
    if items.is_empty() {
        return Err("outer-budget fixture must expose a nonempty selected set".into());
    }
    let snapshot_id = full["snapshot_id"]
        .as_str()
        .ok_or("full page omitted snapshot ID")?
        .to_string();
    let full_bytes = semantic_envelope_bytes(&full_reply, "full page")?;
    let short_full_id_bytes = serde_json::to_vec(&json!("outer-full"))
        .map_err(|error| error.to_string())?
        .len();
    let overhead_without_id =
        frame_bytes(&full_reply, "full page")? - short_full_id_bytes - full_bytes;
    if empty_bytes < 1_400 || full_bytes <= empty_bytes {
        return Err(format!(
            "outer-budget measurement interval drifted: empty={empty_bytes}, full={full_bytes}"
        ));
    }

    // Refusal stimulus: reserve the actual escaped id plus the outer
    // reserve so the id-aware budget excludes even the empty page, while
    // the same envelope passed the old envelope-only checks and the
    // complete response overflowed the wire cap (the RED condition below).
    let refusal_quotes = (RESPONSE_BOUND - OUTER_RESERVE - empty_bytes + 256) / 2;
    let refusal_id = "\"".repeat(refusal_quotes);
    let refusal_id_bytes = serde_json::to_vec(&json!(refusal_id))
        .map(|bytes| bytes.len())
        .map_err(|error| error.to_string())?;
    let request = line(json!({
        "jsonrpc": "2.0",
        "id": refusal_id,
        "method": "tools/call",
        "params": { "name": "ripr_list_gaps", "arguments": {} },
    }))?;
    if request.len() > INPUT_CAP {
        return Err("giant refusal stimulus exceeds the input cap".into());
    }
    if full_bytes + overhead_without_id + refusal_id_bytes <= RESPONSE_BOUND {
        return Err(format!(
            "refusal stimulus missed the old-code wire overflow: full={full_bytes}, overhead={overhead_without_id}, id={refusal_id_bytes}"
        ));
    }
    if RESPONSE_BOUND - OUTER_RESERVE - refusal_id_bytes >= empty_bytes {
        return Err(
            "refusal stimulus must exclude even the empty page from the id-aware budget".into(),
        );
    }
    let refused = session.call_tool(&refusal_id, "ripr_list_gaps", json!({}), timeout)?;
    if refused.get("error").is_some()
        || refused.pointer("/result/isError") != Some(&json!(true))
        || refused.pointer("/result/structuredContent/schema_version")
            != Some(&json!("ripr-mcp-gap-list-v2"))
        || refused.pointer("/result/structuredContent/failure/code")
            != Some(&json!("result_too_large"))
        || refused.pointer("/result/structuredContent/failure/data/current_snapshot_id")
            != Some(&json!(snapshot_id))
    {
        return Err(format!(
            "a long request id must refuse typed before the wire cap: error={:?}, isError={:?}, code={:?}, reply_tail={}",
            refused.get("error"),
            refused.pointer("/result/isError"),
            refused.pointer("/result/structuredContent/failure/code"),
            refused
                .to_string()
                .chars()
                .rev()
                .take(300)
                .collect::<String>()
        ));
    }
    let recovery = refused
        .pointer("/result/structuredContent/failure/recovery")
        .and_then(Value::as_str)
        .ok_or("outer-budget refusal omitted recovery")?;
    if !recovery.contains("ripr://snapshot/{snapshot_id}") || !recovery.contains("ripr_get_gap") {
        return Err(format!(
            "outer-budget refusal lost its bounded identity recovery: {recovery}"
        ));
    }
    if frame_bytes(&refused, "typed refusal")? > RESPONSE_BOUND {
        return Err("the typed refusal itself must fit the wire cap".into());
    }

    // Fit stimulus one notch below the refusal boundary: the identical call
    // must still ship its structured page to this long id, so the
    // reservation cannot quietly refuse fitting responses.
    let fit_quotes = (RESPONSE_BOUND - OUTER_RESERVE - empty_bytes - 512) / 2;
    let fit_id = "\"".repeat(fit_quotes);
    let fit_id_bytes = serde_json::to_vec(&json!(fit_id))
        .map(|bytes| bytes.len())
        .map_err(|error| error.to_string())?;
    if RESPONSE_BOUND - OUTER_RESERVE - fit_id_bytes < empty_bytes {
        return Err("fit stimulus must leave the empty page inside the id-aware budget".into());
    }
    let fitted = session.call_tool(
        &fit_id,
        "ripr_list_gaps",
        json!({ "file": empty_filter }),
        timeout,
    )?;
    let fitted_document = structured_tool_success(&fitted, "outer-budget fit page")?;
    if fitted_document["items"] != json!([])
        || fitted_document["filters"]["file"] != json!(empty_filter)
        || fitted_document["snapshot_id"] != json!(snapshot_id)
    {
        return Err(format!(
            "a fitting call to a long request id lost its structured page: {fitted_document}"
        ));
    }
    if frame_bytes(&fitted, "fitting page")? > RESPONSE_BOUND {
        return Err("the fitting page exceeded the wire cap".into());
    }

    // Resource reads answer one request whose echoed id rides the same wire
    // cap. Size the id-aware budget against the status resource's actually
    // measured envelope: one id excludes it (typed resource refusal where
    // the envelope-only bound shipped the generic fallback), one sits below
    // so the same read still serves.
    let status_reply = session.call(
        "outer-status-short",
        "resources/read",
        json!({ "uri": "ripr://workspace/status" }),
        timeout,
    )?;
    let mut status_result = status_reply
        .get("result")
        .cloned()
        .ok_or("status resource read lost its result")?;
    if let Some(object) = status_result.as_object_mut() {
        object.remove("resultType");
    }
    let status_bytes = serde_json::to_vec(&status_result)
        .map(|bytes| bytes.len())
        .map_err(|error| error.to_string())?;
    if status_bytes < 1_400 {
        return Err(format!(
            "status resource measurement interval drifted: {status_bytes}"
        ));
    }
    let resource_quotes = (RESPONSE_BOUND - OUTER_RESERVE - status_bytes + 256) / 2;
    let resource_refusal_id = "\"".repeat(resource_quotes);
    if status_bytes
        + overhead_without_id
        + serde_json::to_vec(&json!(resource_refusal_id))
            .map(|bytes| bytes.len())
            .map_err(|error| error.to_string())?
        <= RESPONSE_BOUND
    {
        return Err("resource refusal stimulus missed the old-code wire overflow".into());
    }
    let refused_resource = session.call(
        &resource_refusal_id,
        "resources/read",
        json!({ "uri": "ripr://workspace/status" }),
        timeout,
    )?;
    if refused_resource.pointer("/error/data/code") != Some(&json!("result_too_large"))
        || refused_resource.pointer("/error/data/recovery").is_none()
        || refused_resource.get("result").is_some()
    {
        return Err(format!(
            "a long request id must refuse a resource read typed before the wire cap: error={:?}, has_result={}",
            refused_resource.get("error"),
            refused_resource.get("result").is_some()
        ));
    }
    if frame_bytes(&refused_resource, "resource refusal")? > RESPONSE_BOUND {
        return Err("the typed resource refusal itself must fit the wire cap".into());
    }
    let resource_fit_quotes = (RESPONSE_BOUND - OUTER_RESERVE - status_bytes - 512) / 2;
    let fitted_resource = session.call(
        &"\"".repeat(resource_fit_quotes),
        "resources/read",
        json!({ "uri": "ripr://workspace/status" }),
        timeout,
    )?;
    if fitted_resource.get("result").is_none()
        || fitted_resource.get("error").is_some()
        || fitted_resource
            .pointer("/result/contents/0/text")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .is_none()
    {
        return Err(format!(
            "a fitting resource read to a long request id must still serve: {}",
            fitted_resource
                .get("error")
                .map(|error| error.to_string())
                .unwrap_or_default()
        ));
    }
    if frame_bytes(&fitted_resource, "fitting resource")? > RESPONSE_BOUND {
        return Err("the fitting resource read exceeded the wire cap".into());
    }

    let recovered = session.call_tool("outer-recovered", "ripr_list_gaps", json!({}), timeout)?;
    if structured_tool_success(&recovered, "outer-budget recovery")? != full {
        return Err("outer-budget refusals changed the completed snapshot or selection".into());
    }
    Ok(())
}

/// #7768: the original silent drop at the raw stdio boundary. A success
/// document whose complete double-carry envelope exceeds the id-aware
/// envelope budget used to ship as a text-only success —
/// `structuredContent` silently absent although the tool advertises an
/// `outputSchema`. Both stimulus ids are computed here from the
/// `ripr_workspace_status` envelope actually measured on this session's
/// wire: one puts the id-aware budget inside the refusal window (the
/// structured envelope over it, the compact text under it) and must be
/// answered with the typed `result_too_large` refusal carrying structured
/// failure content; a shorter id one notch below keeps the budget above
/// the structured envelope so the same call still ships its structured
/// status. Reverting to the text-only fallback fails the window arm (it
/// ships a success without `structuredContent`); a reservation large
/// enough to refuse fitting envelopes fails the fit arm.
#[test]
fn window_sized_envelope_refuses_typed_instead_of_dropping_structured_content() -> Result<(), String>
{
    const RESPONSE_BOUND: usize = 128 * 1024;
    const INPUT_CAP: usize = 256 * 1024;
    const OUTER_RESERVE: usize = 128;
    let timeout = Duration::from_secs(30);
    let root = install_b4_repair_ready_fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let mut session = SequentialStdio::spawn(&root)?;
    let discovery = session.call("window-discover", "server/discover", json!({}), timeout)?;
    if discovery.get("result").is_none() {
        return Err("window fixture failed discovery".into());
    }

    // Measure the status envelope on the real wire with a short id. The
    // semantic envelope is the reply's result member minus the `resultType`
    // field the SDK adds after the semantic measurement.
    let short = session.call_tool("window-short", "ripr_workspace_status", json!({}), timeout)?;
    let document = structured_tool_success(&short, "window status measure")?;
    let semantic = {
        let mut result = short
            .get("result")
            .cloned()
            .ok_or("window status measure lost its result")?;
        if let Some(object) = result.as_object_mut() {
            object.remove("resultType");
        }
        result
    };
    let structured_bytes = serde_json::to_vec(&semantic)
        .map_err(|error| error.to_string())?
        .len();
    // The text-only envelope the removed fallback used to ship for the
    // same document, measured locally from the double-carry reply.
    let text = semantic
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .ok_or("window status measure lost its text copy")?;
    let text_only_bytes = serde_json::to_vec(&json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
    }))
    .map_err(|error| error.to_string())?
    .len();
    if structured_bytes <= text_only_bytes || text_only_bytes < 1_000 {
        return Err(format!(
            "window measurement interval drifted: structured={structured_bytes}, text_only={text_only_bytes}"
        ));
    }

    // Refusal stimulus: the id-aware budget sits strictly inside the
    // window between the compact-text envelope and the structured one.
    let window_budget = text_only_bytes + (structured_bytes - text_only_bytes) / 2;
    let window_id_bytes = RESPONSE_BOUND - OUTER_RESERVE - window_budget;
    if window_id_bytes + 128 > INPUT_CAP {
        return Err("window refusal stimulus exceeds the input cap".into());
    }
    let window_quotes = (window_id_bytes - 2) / 2;
    let window_id = "\"".repeat(window_quotes);
    let actual_window_id_bytes = serde_json::to_vec(&json!(window_id))
        .map_err(|error| error.to_string())?
        .len();
    let budget = RESPONSE_BOUND - OUTER_RESERVE - actual_window_id_bytes;
    if budget <= text_only_bytes || budget >= structured_bytes {
        return Err(format!(
            "window stimulus missed its interval: budget={budget}, text_only={text_only_bytes}, structured={structured_bytes}"
        ));
    }
    let refused = session.call_tool(&window_id, "ripr_workspace_status", json!({}), timeout)?;
    if refused.get("error").is_some()
        || refused.pointer("/result/isError") != Some(&json!(true))
        || refused.pointer("/result/structuredContent/failure/code")
            != Some(&json!("result_too_large"))
    {
        return Err(format!(
            "a window-sized envelope must refuse typed with structured failure content, not drop structuredContent silently: isError={:?}, has_result={}, has_structured={}, code={:?}",
            refused.pointer("/result/isError"),
            refused.get("result").is_some(),
            refused.pointer("/result/structuredContent").is_some(),
            refused.pointer("/result/structuredContent/failure/code")
        ));
    }
    let recovery = refused
        .pointer("/result/structuredContent/failure/recovery")
        .and_then(Value::as_str)
        .ok_or("window refusal omitted recovery")?;
    if !recovery.contains("no narrower status read") {
        return Err(format!(
            "window refusal lost the status recovery route: {recovery}"
        ));
    }
    if serde_json::to_vec(&refused)
        .map_err(|error| error.to_string())?
        .len()
        + 1
        > RESPONSE_BOUND
    {
        return Err("the window refusal itself must fit the wire cap".into());
    }

    // Fit control one notch below: a shorter id keeps the budget above the
    // structured envelope, so the same call still ships its structured
    // status to this long id.
    let fit_quotes = (RESPONSE_BOUND - OUTER_RESERVE - structured_bytes - 512 - 2) / 2;
    let fit_id = "\"".repeat(fit_quotes);
    let actual_fit_budget = RESPONSE_BOUND
        - OUTER_RESERVE
        - serde_json::to_vec(&json!(fit_id))
            .map_err(|error| error.to_string())?
            .len();
    if actual_fit_budget < structured_bytes {
        return Err(format!(
            "fit stimulus must keep the structured status inside the budget: budget={actual_fit_budget}, structured={structured_bytes}"
        ));
    }
    let fitted = session.call_tool(&fit_id, "ripr_workspace_status", json!({}), timeout)?;
    let fitted_document = structured_tool_success(&fitted, "window fit status")?;
    if fitted_document["schema_version"] != document["schema_version"] {
        return Err("the fitting status lost its document shape".into());
    }
    if serde_json::to_vec(&fitted)
        .map_err(|error| error.to_string())?
        .len()
        + 1
        > RESPONSE_BOUND
    {
        return Err("the fitting status exceeded the wire cap".into());
    }
    Ok(())
}

/// A successful raw-stdio discriminator, including actual continuation replay.
/// Dropping only the forwarded filter after validation must return excluded
/// subjects and fail these literal ID/count oracles. No synthetic snapshot is
/// injected and no WorkspaceSession method is called by this test.
#[test]
fn refreshed_gap_filters_narrow_real_stdio_and_replay_bound_continuation() -> Result<(), String> {
    const DISCOUNT: &str =
        "gap:rust:src/pricing.rs:discount:predicate_boundary:predicate:amount==threshold";
    const FEE: &str = "gap:rust:src/pricing.rs:fee:predicate_boundary:predicate:amount==threshold";
    const UNCOVERED: &str =
        "gap:rust:src/uncovered.rs:accepts:predicate_boundary:predicate:amount==threshold";
    const RETURN: &str =
        "gap:rust:src/uncovered.rs:accepts:return_value:return_value:amount>threshold";
    const PYTHON: &str =
        "gap:python:src/cart.py:accepts:predicate_boundary:predicate:amount>=threshold";
    let timeout = Duration::from_mins(1);
    let root = install_gap_filter_fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let (binary, digest) = Observation::executable_custody(Path::new(env!("CARGO_BIN_EXE_ripr")))?;
    let fixture_tree =
        common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD^{tree}"])?;
    eprintln!(
        "filter control fixture_tree={} binary={} sha256={digest}",
        fixture_tree.trim(),
        binary.display()
    );
    let mut session = SequentialStdio::spawn(&root)?;
    let discovery = session.call("discover-filters", "server/discover", json!({}), timeout)?;
    if discovery.get("result").is_none() {
        return Err(format!("discovery failed: {discovery}"));
    }
    let refresh = session.call_tool(
        "refresh-filters",
        "ripr_refresh",
        json!({}),
        Duration::from_mins(3),
    )?;
    let refresh = structured_tool_success(&refresh, "filter refresh")?;
    if refresh["attempt"]["state"] != "completed"
        || refresh["snapshot"]["outcome_kind"] != "complete_with_findings"
    {
        return Err(format!(
            "fixture must complete analysis before filter assertions: {refresh}"
        ));
    }
    let all = session.call_tool("all-gaps", "ripr_list_gaps", json!({}), timeout)?;
    let all = structured_tool_success(&all, "unfiltered list")?;
    let snapshot_id = all["snapshot_id"]
        .as_str()
        .ok_or("list omitted snapshot ID")?;
    let mut all_ids = vec![DISCOUNT, FEE, UNCOVERED, RETURN];
    let mut unready = vec![UNCOVERED, RETURN];
    if cfg!(feature = "lang-python") {
        all_ids.insert(0, PYTHON);
        unready.insert(0, PYTHON);
    }
    let selected = all_ids.len();
    check_filtered_gap_page(&all, &all_ids, selected, selected, snapshot_id)?;
    for item in all["items"].as_array().ok_or("list omitted items")? {
        let id = item["canonical_id"].as_str().ok_or("item omitted ID")?;
        let ready = [DISCOUNT, FEE].contains(&id);
        if item["repair_packet_ready"] != ready
            || item["repair_ineligibility"]
                != if ready {
                    Value::Null
                } else {
                    json!("fix_site_not_established")
                }
            || item["class"] != if ready { "exposed" } else { "no_static_path" }
        {
            return Err(format!(
                "fixture lost distinct class/readiness subjects: {item}"
            ));
        }
        let gap = session.call_tool(
            "readiness",
            "ripr_get_gap",
            json!({"canonical_id": id, "snapshot_id": snapshot_id}),
            timeout,
        )?;
        let gap = structured_tool_success(&gap, "get gap readiness")?;
        if gap.pointer("/item/readiness/repair_packet_ready") != Some(&json!(ready)) {
            return Err(format!("summary and full-gap readiness disagree: {gap}"));
        }
    }

    // Frozen pre-feature v1 success schema, from a7ed66f6e. Its closed top
    // level rejects the two added fields even when they are called additive.
    let v1: Value = serde_json::from_str(include_str!("fixtures/mcp-gap-list-v1.schema.json"))
        .map_err(|error| error.to_string())?;
    let properties = v1["properties"]
        .as_object()
        .ok_or("v1 fixture omitted properties")?;
    let required = v1["required"]
        .as_array()
        .ok_or("v1 fixture omitted required")?;
    let accepts_v1_shape = |document: &Value| {
        document.as_object().is_some_and(|object| {
            object.keys().all(|key| properties.contains_key(key))
                && required
                    .iter()
                    .all(|key| key.as_str().is_some_and(|key| object.contains_key(key)))
        })
    };
    let mut legacy = all.clone();
    let legacy_object = legacy.as_object_mut().ok_or("list must be an object")?;
    legacy_object.remove("filtered");
    legacy_object.remove("filters");
    legacy_object.insert("schema_version".into(), json!("ripr-mcp-gap-list-v1"));
    if v1["additionalProperties"] != false || !accepts_v1_shape(&legacy) || accepts_v1_shape(&all) {
        return Err(
            "strict v1 top-level closure must accept legacy keys and reject the extended envelope"
                .into(),
        );
    }

    let mut cases = vec![
        (json!({"file": "./src/pricing.rs"}), vec![DISCOUNT, FEE]),
        (json!({"file": "pricing.rs"}), vec![DISCOUNT, FEE]),
        (json!({"file": "src/uncovered.rs"}), vec![UNCOVERED, RETURN]),
        (json!({"class": "exposed"}), vec![DISCOUNT, FEE]),
        (json!({"class": "no_static_path"}), unready.clone()),
        (
            json!({"language": "rust"}),
            vec![DISCOUNT, FEE, UNCOVERED, RETURN],
        ),
        (json!({"repair_ready": true}), vec![DISCOUNT, FEE]),
        (json!({"repair_ready": false}), unready),
        (
            json!({"file": "src/pricing.rs", "class": "exposed", "language": "rust", "repair_ready": true}),
            vec![DISCOUNT, FEE],
        ),
        (
            json!({"file": "src/pricing.rs", "class": "no_static_path"}),
            vec![],
        ),
        (json!({"class": "unknown-class"}), vec![]),
        (json!({"language": "unknown-language"}), vec![]),
        (json!({"file": "missing.rs"}), vec![]),
    ];
    if cfg!(feature = "lang-python") {
        cases.push((json!({"language": "python"}), vec![PYTHON]));
    }
    for (arguments, expected) in cases {
        let reply = session.call_tool("filtered", "ripr_list_gaps", arguments.clone(), timeout)?;
        let document = structured_tool_success(&reply, "filtered list")?;
        check_filtered_gap_page(&document, &expected, selected, expected.len(), snapshot_id)?;
        for field in ["file", "class", "language", "repair_ready"] {
            if document["filters"][field] != arguments[field] {
                return Err(format!("filter echo lost {field}: {document}"));
            }
        }
        if document["page"]["has_more"] != false || document["page"]["next_offset"] != Value::Null {
            return Err(format!(
                "complete filtered page must close its window: {document}"
            ));
        }
    }
    let first_args = json!({"file":"src/pricing.rs", "class":"exposed", "language":"rust", "repair_ready":true, "limit":1, "snapshot_id":snapshot_id});
    let first = session.call_tool("filtered-first", "ripr_list_gaps", first_args, timeout)?;
    let first = structured_tool_success(&first, "first filtered page")?;
    check_filtered_gap_page(&first, &[DISCOUNT], selected, 2, snapshot_id)?;
    let next = first
        .pointer("/continuation/next_page")
        .ok_or("first page omitted bound continuation")?;
    let expected_args = json!({"snapshot_id":snapshot_id, "offset":1, "limit":1, "file":"src/pricing.rs", "class":"exposed", "language":"rust", "repair_ready":true});
    if first["page"]
        != json!({"offset":0, "limit":1, "returned":1, "has_more":true, "next_offset":1})
        || next["tool"] != "ripr_list_gaps"
        || next["arguments"] != expected_args
    {
        return Err(format!(
            "filtered continuation lost its exact window/binding: {first}"
        ));
    }
    let next_tool = next["tool"].as_str().ok_or("continuation omitted tool")?;
    let replay = session.call_tool(
        "filtered-replay",
        next_tool,
        next["arguments"].clone(),
        timeout,
    )?;
    let replay = structured_tool_success(&replay, "replayed filtered page")?;
    check_filtered_gap_page(&replay, &[FEE], selected, 2, snapshot_id)?;
    if replay["page"]
        != json!({"offset":1, "limit":1, "returned":1, "has_more":false, "next_offset":null})
        || replay.pointer("/continuation/next_page").is_some()
    {
        return Err(format!(
            "filtered replay must finish exactly once: {replay}"
        ));
    }
    let status = session.call_tool("filter-status", "ripr_workspace_status", json!({}), timeout)?;
    let status = structured_tool_success(&status, "status after filters")?;
    if status.pointer("/session/last_completed_snapshot/snapshot_id") != Some(&json!(snapshot_id)) {
        return Err(format!(
            "filter reads changed the completed snapshot: {status}"
        ));
    }
    // A different committed input replaces the snapshot. Replay the actual
    // old continuation unchanged; no fabricated stale identity is supplied.
    std::fs::write(
        root.join("src/uncovered.rs"),
        "pub fn accepts(amount: i32, threshold: i32) -> bool {\n    amount < threshold\n}\n",
    )
    .map_err(|error| error.to_string())?;
    commit_fixture(&root, "replace uncovered predicate")?;
    let replaced = session.call_tool(
        "replace-refresh",
        "ripr_refresh",
        json!({}),
        Duration::from_mins(3),
    )?;
    let replaced = structured_tool_success(&replaced, "replacement refresh")?;
    if replaced["attempt"]["state"] != "completed"
        || replaced["snapshot"]["snapshot_id"] == snapshot_id
    {
        return Err(format!(
            "changed committed input must replace the snapshot: {replaced}"
        ));
    }
    let stale = session.call_tool(
        "stale-replay",
        next_tool,
        next["arguments"].clone(),
        timeout,
    )?;
    if stale.pointer("/result/isError") != Some(&json!(true))
        || stale.pointer("/result/structuredContent/failure/code") != Some(&json!("stale_snapshot"))
    {
        return Err(format!(
            "old generated continuation must fail stale: {stale}"
        ));
    }
    Ok(())
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

/// Keep the wording tied to actual input selection: the same dirty fixture
/// yields CLI findings but a committed-only MCP snapshot with scope disclosure.
#[cfg(feature = "lang-rust")]
#[test]
fn dirty_tree_cli_and_mcp_retain_their_distinct_inputs() -> Result<(), String> {
    let root = install_b4_repair_ready_fixture()?;
    let _cleanup = FixtureGuard { root: root.clone() };
    std::fs::write(root.join(".gitignore"), "/target/\n").map_err(|error| error.to_string())?;
    commit_fixture(&root, "ignore generated analysis artifacts")?;
    fixture_git_ok(&root, &["update-ref", "refs/heads/main", "HEAD"])?;
    let committed_diff = common::fixture_git::fixture_git_output(&root, &["diff", "main...HEAD"])?;
    if !committed_diff.trim().is_empty() {
        return Err("fixture must have an empty committed default-branch diff".to_string());
    }
    let source = root.join("src/lib.rs");
    let committed = std::fs::read_to_string(&source).map_err(|error| error.to_string())?;
    std::fs::write(&source, committed.replace("amount - 10", "amount - 20"))
        .map_err(|error| error.to_string())?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    std::fs::write(
        &source,
        committed.replace("amount - 10", "amount - 20").replace(
            "amount > discount_threshold",
            "amount >= discount_threshold",
        ),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/untracked.rs"),
        "pub fn untracked() -> i32 { 42 }\n",
    )
    .map_err(|error| error.to_string())?;
    let before = common::fixture_git::fixture_git_output(&root, &["status", "--porcelain"])?;
    if !before.contains("MM src/lib.rs") || !before.contains("?? src/untracked.rs") {
        return Err(format!(
            "fixture lost staged/unstaged/untracked subjects: {before}"
        ));
    }
    let before_head = common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?;
    let before_diff =
        common::fixture_git::fixture_git_output(&root, &["diff", "--binary", "HEAD"])?;
    let before_index =
        common::fixture_git::fixture_git_output(&root, &["diff", "--cached", "--binary"])?;
    let before_untracked =
        std::fs::read(root.join("src/untracked.rs")).map_err(|error| error.to_string())?;
    let check = |committed_only: bool| -> Result<Value, String> {
        let mut command = worktree_ripr();
        command
            .args(["check", "--format", "json", "--root"])
            .arg(&root);
        if committed_only {
            command.arg("--committed");
        }
        let output = command.output().map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
    };
    let live = check(false)?;
    if live.pointer("/head/source").and_then(Value::as_str) != Some("working_tree")
        || live
            .get("findings")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    {
        return Err(format!(
            "dirty default CLI must analyze real findings: {live}"
        ));
    }
    let committed_cli = check(true)?;
    if committed_cli
        .get("findings")
        .and_then(Value::as_array)
        .is_none_or(|items| !items.is_empty())
        || committed_cli.get("unanalyzed_working_tree") != Some(&json!(true))
    {
        return Err(format!(
            "--committed must exclude dirty edits and disclose them: {committed_cli}"
        ));
    }
    let mut session = SequentialStdio::spawn(&root)?;
    let timeout = Duration::from_mins(1);
    session.call("discover", "server/discover", json!({}), timeout)?;
    let refresh = session.call_tool("refresh", "ripr_refresh", json!({}), timeout)?;
    let refresh = structured_tool_success(&refresh, "ripr_refresh")?;
    if refresh.pointer("/attempt/state").and_then(Value::as_str) != Some("completed")
        || refresh
            .pointer("/snapshot/outcome_kind")
            .and_then(Value::as_str)
            != Some("no_scope")
        || refresh
            .pointer("/snapshot/total_items")
            .and_then(Value::as_u64)
            != Some(0)
        || refresh.pointer("/scope/analyzed").and_then(Value::as_str)
            != Some("committed default-branch diff")
        || refresh
            .pointer("/scope/untracked_edits/total")
            .and_then(Value::as_u64)
            != Some(1)
    {
        return Err(format!(
            "MCP must retain committed-only no_scope with dirty disclosure: {refresh}"
        ));
    }
    let status = session.call_tool("status", "ripr_workspace_status", json!({}), timeout)?;
    let status = structured_tool_success(&status, "ripr_workspace_status")?;
    if status.pointer("/session/scope") != refresh.get("scope") {
        return Err(format!("status lost refresh scope disclosure: {status}"));
    }
    drop(session);
    let after = common::fixture_git::fixture_git_output(&root, &["status", "--porcelain"])?;
    let after_head = common::fixture_git::fixture_git_output(&root, &["rev-parse", "HEAD"])?;
    let after_diff = common::fixture_git::fixture_git_output(&root, &["diff", "--binary", "HEAD"])?;
    let after_index =
        common::fixture_git::fixture_git_output(&root, &["diff", "--cached", "--binary"])?;
    let after_untracked =
        std::fs::read(root.join("src/untracked.rs")).map_err(|error| error.to_string())?;
    if before != after
        || before_head != after_head
        || before_diff != after_diff
        || before_index != after_index
        || before_untracked != after_untracked
    {
        return Err(format!(
            "analysis changed the dirty fixture: before={before:?}, after={after:?}"
        ));
    }
    Ok(())
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

fn read_resource_document(
    session: &mut SequentialStdio,
    id: &str,
    uri: &str,
    timeout: Duration,
) -> Result<Value, String> {
    let reply = session.call(id, "resources/read", json!({ "uri": uri }), timeout)?;
    let text = reply
        .pointer("/result/contents/0/text")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("resource {uri} omitted JSON text: {reply}"))?;
    serde_json::from_str(text)
        .map_err(|error| format!("resource {uri} text is not JSON: {error}: {text}"))
}

/// After refresh on the B4 repair-ready fixture, every tool/resource pair
/// returns the same document: `ripr_get_gap` with `ripr://gap/{id}`,
/// `ripr_get_repair_attempt` with `ripr://repair-attempt/{id}`,
/// `ripr_get_receipt_status` with `ripr://receipt/{id}`, and
/// `ripr_get_repair_card` with `ripr://repair-card/{id}`. The snapshot
/// resource has no tool twin, so it pins its envelope instead
/// (`ripr-mcp-snapshot-v1`, identity, typed outcome). A drift in URI
/// parsing, resource envelopes, or error mapping fails this control (#7145).
#[test]
fn resource_reads_match_tools_after_refresh() -> Result<(), String> {
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
            "B4 refresh must yield a gap before the equivalence control can run: {refresh}"
        ));
    }

    let list = session.call_tool("list", "ripr_list_gaps", json!({}), REPLY_TIMEOUT)?;
    let list = structured_tool_success(&list, "ripr_list_gaps")?;
    let snapshot_id = list
        .pointer("/snapshot_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("ripr_list_gaps omitted snapshot_id: {list}"))?
        .to_string();
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

    // Snapshot: envelope pin (no tool twin exists).
    let snapshot = read_resource_document(
        &mut session,
        "snapshot-resource",
        &format!("ripr://snapshot/{snapshot_id}"),
        REPLY_TIMEOUT,
    )?;
    if snapshot.pointer("/schema_version").and_then(Value::as_str) != Some("ripr-mcp-snapshot-v1") {
        return Err(format!(
            "snapshot resource lost ripr-mcp-snapshot-v1: {snapshot}"
        ));
    }
    if snapshot.pointer("/snapshot_id").and_then(Value::as_str) != Some(snapshot_id.as_str()) {
        return Err(format!("snapshot resource lost its identity: {snapshot}"));
    }
    if snapshot
        .pointer("/outcome/kind")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(format!(
            "snapshot resource lost the typed outcome: {snapshot}"
        ));
    }

    // Gap pair.
    let gap_tool = session.call_tool(
        "gap-tool",
        "ripr_get_gap",
        json!({ "canonical_id": canonical_id }),
        REPLY_TIMEOUT,
    )?;
    let gap_tool = structured_tool_success(&gap_tool, "ripr_get_gap")?;
    let gap_resource = read_resource_document(
        &mut session,
        "gap-resource",
        &format!("ripr://gap/{canonical_id}"),
        REPLY_TIMEOUT,
    )?;
    if gap_tool != gap_resource {
        return Err(format!(
            "gap tool and resource diverged:\ntool={gap_tool}\nresource={gap_resource}"
        ));
    }

    // Card pair.
    let card_tool = session.call_tool(
        "card-tool",
        "ripr_get_repair_card",
        json!({ "canonical_id": canonical_id }),
        REPLY_TIMEOUT,
    )?;
    let card_tool = structured_tool_success(&card_tool, "ripr_get_repair_card")?;
    let card_resource = read_resource_document(
        &mut session,
        "card-resource",
        &format!("ripr://repair-card/{canonical_id}"),
        REPLY_TIMEOUT,
    )?;
    if card_tool != card_resource {
        return Err(format!(
            "repair-card tool and resource diverged:\ntool={card_tool}\nresource={card_resource}"
        ));
    }

    // Encoded id reads (#8736): canonical ids embed raw workspace-relative
    // paths, so a URI-emitting client percent-encodes characters like `:`,
    // `/`, space, and `#`. The resource route must decode the captured
    // segment once and resolve to the same documents the raw id addressed.
    let encoded_id = canonical_id
        .replace('%', "%25")
        .replace(':', "%3A")
        .replace('/', "%2F")
        .replace(' ', "%20")
        .replace('#', "%23");
    if encoded_id == canonical_id {
        return Err("B4 canonical id must contain encodable characters".into());
    }
    let gap_encoded = read_resource_document(
        &mut session,
        "gap-resource-encoded",
        &format!("ripr://gap/{encoded_id}"),
        REPLY_TIMEOUT,
    )?;
    if gap_encoded != gap_tool {
        return Err(format!(
            "encoded gap resource id did not decode to the same document:\nencoded={gap_encoded}\ntool={gap_tool}"
        ));
    }
    let card_encoded = read_resource_document(
        &mut session,
        "card-resource-encoded",
        &format!("ripr://repair-card/{encoded_id}"),
        REPLY_TIMEOUT,
    )?;
    if card_encoded != card_tool {
        return Err(format!(
            "encoded repair-card resource id did not decode to the same document:\nencoded={card_encoded}\ntool={card_tool}"
        ));
    }

    // Attempt pair: prepare the B4 item, then read both routes.
    let prepared = session.call_tool(
        "prepare",
        "ripr_prepare_repair",
        json!({ "canonical_id": canonical_id }),
        REPLY_TIMEOUT,
    )?;
    let prepared = structured_tool_success(&prepared, "ripr_prepare_repair")?;
    let attempt_id = prepared
        .pointer("/attempt/attempt_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("B4 prepare must yield an attempt: {prepared}"))?
        .to_string();
    let attempt_tool = session.call_tool(
        "attempt-tool",
        "ripr_get_repair_attempt",
        json!({ "attempt_id": attempt_id }),
        REPLY_TIMEOUT,
    )?;
    let attempt_tool = structured_tool_success(&attempt_tool, "ripr_get_repair_attempt")?;
    let attempt_resource = read_resource_document(
        &mut session,
        "attempt-resource",
        &format!("ripr://repair-attempt/{attempt_id}"),
        REPLY_TIMEOUT,
    )?;
    if attempt_tool != attempt_resource {
        return Err(format!(
            "repair-attempt tool and resource diverged:\ntool={attempt_tool}\nresource={attempt_resource}"
        ));
    }

    // Receipt pair: session receipt ids are attempt-bound.
    let receipt_tool = session.call_tool(
        "receipt-tool",
        "ripr_get_receipt_status",
        json!({ "receipt_id": attempt_id }),
        REPLY_TIMEOUT,
    )?;
    let receipt_tool = structured_tool_success(&receipt_tool, "ripr_get_receipt_status")?;
    let receipt_resource = read_resource_document(
        &mut session,
        "receipt-resource",
        &format!("ripr://receipt/{attempt_id}"),
        REPLY_TIMEOUT,
    )?;
    if receipt_tool != receipt_resource {
        return Err(format!(
            "receipt tool and resource diverged:\ntool={receipt_tool}\nresource={receipt_resource}"
        ));
    }

    // Miss mapping: unknown ids fail closed with typed data codes.
    for (id, uri, kind) in [
        ("gap-miss", "ripr://gap/no-such-item", "gap"),
        (
            "card-miss",
            "ripr://repair-card/no-such-item",
            "repair-card",
        ),
    ] {
        let miss = session.call(id, "resources/read", json!({ "uri": uri }), REPLY_TIMEOUT)?;
        let code = miss.pointer("/error/data/code").and_then(Value::as_str);
        if code != Some("item_not_found") {
            return Err(format!("{kind} resource miss lost item_not_found: {miss}"));
        }
    }
    Ok(())
}
