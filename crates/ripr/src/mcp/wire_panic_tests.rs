//! #11116: through the real stdio session, a panic in a non-refresh tool
//! handler is answered as a typed tool failure and the next request on the
//! same connection is served.
use super::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Removes the fixture root on every exit path, including early `Err` returns.
struct Cleanup(std::path::PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn panicking_tool_handler_keeps_the_stdio_session_serving() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-mcp-wire-panic-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let _cleanup = Cleanup(root.clone());
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"mcp-wire-panic\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/lib.rs"), "pub fn one() -> u8 { 1 }\n")
        .map_err(|error| error.to_string())?;
    let init = std::process::Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .map_err(|error| error.to_string())?;
    if !init.status.success() {
        return Err("git init failed for the wire fixture".to_string());
    }
    let (status, analysis_root) = WorkspaceStatus::resolve_with_root(Some(root.clone()));
    if analysis_root.is_none() {
        return Err("wire fixture root did not resolve; the injection would be keyed wrong".into());
    }
    let armed_root = analysis_root
        .clone()
        .ok_or_else(|| "wire fixture root vanished".to_string())?;
    let armed = super::super::panic_containment::injected_tool_panic::arm(
        &armed_root,
        super::super::protocol::LIST_GAPS_TOOL_NAME,
    );
    let (mut input, source) = tokio::io::duplex(64 * 1024);
    let (sink, output) = tokio::io::duplex(64 * 1024);
    let session = tokio::spawn(serve(source, sink, status, analysis_root));
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name":"panic-test","version":"1"}
    });
    let mut output = BufReader::new(output);
    let mut replies: Vec<Value> = Vec::new();
    for (id, tool) in [(1_u64, "ripr_list_gaps"), (2, "ripr_workspace_status")] {
        let mut bytes = serde_json::to_vec(&json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"_meta": meta, "name": tool, "arguments": {}}
        }))
        .map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        input
            .write_all(&bytes)
            .await
            .map_err(|error| error.to_string())?;
        let mut line = Vec::new();
        let count = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            output.read_until(b'\n', &mut line),
        )
        .await
        .map_err(|_elapsed| format!("no reply to {tool}: the session stopped serving"))?
        .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err(format!("session ended before replying to {tool}"));
        }
        replies.push(serde_json::from_slice(&line).map_err(|error| error.to_string())?);
    }
    drop(input);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), session).await;
    drop(armed);
    let [failed, served] = replies.as_slice() else {
        return Err("expected exactly two replies".to_string());
    };
    if failed.pointer("/result/isError") != Some(&json!(true))
        || failed.pointer("/result/structuredContent/failure/code")
            != Some(&json!("analysis_failed"))
        || !failed.to_string().contains("ripr_list_gaps")
    {
        return Err(format!("panic was not a typed tool failure: {failed}"));
    }
    if served.get("id") != Some(&json!(2))
        || served.get("result").is_none()
        || served.pointer("/result/isError") == Some(&json!(true))
    {
        return Err(format!("session did not keep serving: {served}"));
    }
    Ok(())
}
