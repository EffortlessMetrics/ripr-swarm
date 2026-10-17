//! Generic-client `window/showMessage` for refresh failures (#8583).
//!
//! Drives the retained `report_refresh_failure_after` producer through the
//! real `Client` socket so the tests see the framed LSP notifications, not a
//! parallel helper. `ripr/analysisStatus` is a `Client::send_notification`
//! and tower-lsp suppresses it until `initialize`; these tests therefore
//! discriminate on `window/logMessage` / `window/showMessage`, which use the
//! unchecked client path both before and after initialize.

use super::{Backend, bounded_failure_message, refresh_failed_log_message, refresh_failure_notice};
use crate::analysis::cancellation::AnalysisCancellationToken;
use crate::lsp::client_features::{ClientFeatureProfile, RiprEditorClientCapabilities};
use crate::lsp::config::LspAnalysisConfig;
use crate::lsp::git_inputs::ResolvedGitInputs;
use crate::lsp::input_identity::LspAnalysisInputIdentity;
use crate::lsp::refresh_scheduler::{
    RefreshAttemptOutcome, RefreshReason, RefreshRequest, RefreshScope,
};
use crate::lsp::state::AnalysisFailureKind;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tower_lsp_server::{LspService, Server};

struct ShowMessageHarness {
    service: LspService<Backend>,
    runtime: tokio::runtime::Runtime,
    messages: Arc<Mutex<Vec<Value>>>,
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to start test runtime: {err}"))
}

fn spawn_harness() -> Result<ShowMessageHarness, String> {
    let runtime = runtime()?;
    let (service, socket) = LspService::new(|client| Backend::new(client, PathBuf::from(".")));
    let (idle_service, _idle_socket) =
        LspService::new(|client| Backend::new(client, PathBuf::from(".")));
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let (client_read, _client_write) = tokio::io::split(client_io);
    let (server_read, server_write) = tokio::io::split(server_io);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&messages);
    runtime.spawn(async move {
        Server::new(server_read, server_write, socket)
            .serve(idle_service)
            .await;
    });
    runtime.spawn(async move {
        let mut reader = client_read;
        loop {
            match read_lsp_message(&mut reader).await {
                Ok(message) => {
                    if let Ok(mut messages) = collected.lock() {
                        messages.push(message);
                    }
                }
                Err(_) => break,
            }
        }
    });
    Ok(ShowMessageHarness {
        service,
        runtime,
        messages,
    })
}

fn test_request(generation: u64) -> RefreshRequest {
    let root = PathBuf::from("/workspace");
    let config = LspAnalysisConfig::default();
    RefreshRequest {
        generation,
        authority_epoch: 0,
        input_identity: LspAnalysisInputIdentity::from_refresh_inputs(
            root.clone(),
            generation,
            &config,
        ),
        git_inputs: ResolvedGitInputs::resolve(&root, None, None),
        root,
        config,
        workspace_revision: generation,
        scope: RefreshScope::Interactive,
        reason: RefreshReason::DidSave,
        cancellation: AnalysisCancellationToken::new(),
    }
}

fn vscode_editor_profile() -> ClientFeatureProfile {
    let mut profile = ClientFeatureProfile::unsupported();
    profile.ripr_editor = Some(RiprEditorClientCapabilities {
        version: "test".to_string(),
        commands: Vec::new(),
        guarded_test_edit: false,
    });
    profile
}

impl ShowMessageHarness {
    fn backend(&self) -> &Backend {
        self.service.inner()
    }

    fn install_profile(&self, profile: ClientFeatureProfile) {
        if let Ok(mut features) = self.backend().client_features.lock() {
            *features = profile;
        }
    }

    fn snapshot(&self) -> Result<Vec<Value>, String> {
        self.messages
            .lock()
            .map(|messages| messages.clone())
            .map_err(|err| format!("message lock poisoned: {err}"))
    }

    fn wait_until(
        &self,
        predicate: impl Fn(&[Value]) -> bool,
        timeout: Duration,
    ) -> Result<Vec<Value>, String> {
        let deadline = tokio::time::Instant::now() + timeout;
        self.runtime.block_on(async {
            loop {
                let snapshot = self.snapshot()?;
                if predicate(&snapshot) {
                    return Ok(snapshot);
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(format!(
                        "timed out waiting for LSP traffic, saw {snapshot:?}"
                    ));
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
    }

    fn report_failure(&self, generation: u64, message: &str) {
        let request = test_request(generation);
        self.runtime
            .block_on(self.backend().report_refresh_failure_after(
                &request,
                message.to_string(),
                Duration::from_millis(25),
                AnalysisFailureKind::AnalysisError,
            ));
    }

    fn record_published(&self, generation: u64) {
        self.backend()
            .record_health_outcome(&test_request(generation), RefreshAttemptOutcome::Published);
    }
}

fn method_is(message: &Value, method: &str) -> bool {
    message.get("method").and_then(Value::as_str) == Some(method)
}

fn show_messages(messages: &[Value]) -> Vec<&Value> {
    messages
        .iter()
        .filter(|message| method_is(message, "window/showMessage"))
        .collect()
}

fn log_messages(messages: &[Value]) -> Vec<&Value> {
    messages
        .iter()
        .filter(|message| method_is(message, "window/logMessage"))
        .collect()
}

fn notification_text(message: &Value) -> &str {
    message
        .get("params")
        .and_then(|params| params.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn warning_type(message: &Value) -> Option<i64> {
    message
        .get("params")
        .and_then(|params| params.get("type"))
        .and_then(Value::as_i64)
}

async fn read_lsp_message<R>(reader: &mut R) -> Result<Value, String>
where
    R: AsyncRead + Unpin,
{
    let mut header = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        reader
            .read_exact(&mut byte)
            .await
            .map_err(|err| format!("failed to read LSP header: {err}"))?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header =
        std::str::from_utf8(&header).map_err(|err| format!("invalid LSP header UTF-8: {err}"))?;
    let content_length = header
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .ok_or_else(|| "missing LSP Content-Length header".to_string())?
        .parse::<usize>()
        .map_err(|err| format!("invalid LSP Content-Length header: {err}"))?;
    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|err| format!("failed to read LSP body: {err}"))?;
    serde_json::from_slice(&body).map_err(|err| format!("failed to decode LSP message: {err}"))
}

#[test]
fn refresh_failure_notice_bounds_and_redacts_like_analysis_status() {
    let message = format!(
        "failed reading {} {}",
        "/tmp/ripr-secret-config.toml",
        "x".repeat(400)
    );
    let shown = refresh_failure_notice(&message);
    let bounded = bounded_failure_message(&format!("ripr analysis failed: {message}"));
    assert_eq!(shown, bounded);
    assert!(shown.contains("<path>"), "{shown}");
    assert!(
        !shown.contains("/tmp/ripr-secret-config.toml"),
        "showMessage must redact path tokens: {shown}"
    );
    assert!(shown.ends_with('…'), "{shown}");
    assert_eq!(
        shown.chars().count(),
        crate::lsp::component_outcome::CLIENT_VISIBLE_MESSAGE_CHAR_BOUND,
        "{shown}"
    );
}

#[test]
fn generic_client_is_shown_a_warning_on_the_first_refresh_failure() -> Result<(), String> {
    let harness = spawn_harness()?;
    harness.report_failure(1, "analysis crashed at /workspace/src/pricing.rs");
    let messages = harness.wait_until(
        |messages| !show_messages(messages).is_empty() && !log_messages(messages).is_empty(),
        Duration::from_secs(5),
    )?;
    let shown = show_messages(&messages);
    if shown.len() != 1 {
        return Err(format!("expected one showMessage, got {shown:?}"));
    }
    if warning_type(shown[0]) != Some(2) {
        return Err(format!("showMessage must be Warning: {}", shown[0]));
    }
    let text = notification_text(shown[0]);
    let expected = refresh_failure_notice("analysis crashed at /workspace/src/pricing.rs");
    if text != expected {
        return Err(format!("unexpected showMessage text: {text}"));
    }
    if text.contains("/workspace") {
        return Err(format!("showMessage leaked a path: {text}"));
    }
    let logs = log_messages(&messages);
    if logs
        .iter()
        .map(|message| notification_text(message))
        .all(|text| {
            text != refresh_failed_log_message(
                "analysis crashed at /workspace/src/pricing.rs",
                Duration::from_millis(25),
            )
        })
    {
        return Err(format!(
            "generic clients must still receive the log: {logs:?}"
        ));
    }
    Ok(())
}

#[test]
fn generic_client_is_not_flooded_by_repeated_refresh_failures() -> Result<(), String> {
    let harness = spawn_harness()?;
    harness.report_failure(1, "first crash");
    harness.report_failure(2, "second crash");
    let messages = harness.wait_until(
        |messages| log_messages(messages).len() >= 2,
        Duration::from_secs(5),
    )?;
    let shown = show_messages(&messages);
    if shown.len() != 1 {
        return Err(format!(
            "repeated failures must not send a second showMessage, got {shown:?}"
        ));
    }
    if notification_text(shown[0]) != refresh_failure_notice("first crash") {
        return Err(format!(
            "the one warning must be the first transition: {}",
            notification_text(shown[0])
        ));
    }
    Ok(())
}

#[test]
fn successful_refresh_resets_generic_client_failure_warning() -> Result<(), String> {
    let harness = spawn_harness()?;
    harness.report_failure(1, "first crash");
    harness.wait_until(
        |messages| show_messages(messages).len() == 1,
        Duration::from_secs(5),
    )?;
    harness.record_published(2);
    harness.report_failure(3, "later crash");
    let messages = harness.wait_until(
        |messages| show_messages(messages).len() >= 2,
        Duration::from_secs(5),
    )?;
    let shown = show_messages(&messages);
    if shown.len() != 2 {
        return Err(format!(
            "a success must allow the next failure to warn again, got {shown:?}"
        ));
    }
    if notification_text(shown[1]) != refresh_failure_notice("later crash") {
        return Err(format!(
            "the second warning must be the post-success failure: {}",
            notification_text(shown[1])
        ));
    }
    Ok(())
}

#[test]
fn vscode_client_does_not_receive_refresh_failure_show_message() -> Result<(), String> {
    let harness = spawn_harness()?;
    harness.install_profile(vscode_editor_profile());
    harness.report_failure(1, "analysis crashed at /workspace/src/pricing.rs");
    harness.wait_until(
        |messages| !log_messages(messages).is_empty(),
        Duration::from_secs(5),
    )?;
    harness.runtime.block_on(async {
        tokio::time::sleep(Duration::from_millis(200)).await;
    });
    let messages = harness.snapshot()?;
    let shown = show_messages(&messages);
    if !shown.is_empty() {
        return Err(format!(
            "VS Code must keep rendering refresh failures from analysisStatus only, got {shown:?}"
        ));
    }
    if log_messages(&messages).is_empty() {
        return Err(format!(
            "VS Code must still receive the refresh-failure log: {messages:?}"
        ));
    }
    Ok(())
}
