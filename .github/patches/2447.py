from pathlib import Path

TARGET = Path("crates/ripr/tests/lsp_lifecycle.rs")
text = TARGET.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    text = text.replace(old, new, 1)


replace_once(
    "const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);\n",
    "const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);\n"
    "/// The compatibility journey's explicit `ripr.refresh` request runs the\n"
    "/// non-deferred full seam inventory. Keep cheap lifecycle requests on the\n"
    "/// 15-second fail-fast budget, but give this one bounded demand path enough\n"
    "/// time on a cold Windows hosted runner (#2447).\n"
    "const FULL_REFRESH_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);\n",
    "full-refresh timeout constant",
)

replace_once(
    '''    fn request(
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
''',
    '''    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.request_with_timeout(method, params, RESPONSE_TIMEOUT)
    }

    fn request_with_timeout(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
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
        self.await_response(id, timeout)
    }
''',
    "request-specific timeout seam",
)

replace_once(
    '''    let refresh = session.request(
        "workspace/executeCommand",
        serde_json::json!({"command": "ripr.refresh", "arguments": []}),
    )?;
''',
    '''    let refresh = session.request_with_timeout(
        "workspace/executeCommand",
        serde_json::json!({"command": "ripr.refresh", "arguments": []}),
        FULL_REFRESH_RESPONSE_TIMEOUT,
    )?;
''',
    "explicit refresh timeout call",
)

TARGET.write_text(text, encoding="utf-8")
