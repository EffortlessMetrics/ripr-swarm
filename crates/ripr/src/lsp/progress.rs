//! Standard LSP work-done progress for accepted analysis requests (#1971).
//!
//! Every accepted refresh generation gets one progress token
//! (`ripr-analysis-{generation}`) created via `window/workDoneProgress/create`
//! and driven by `$/progress` notifications:
//!
//! - begin (`queued` for a coalesced request, `analyzing` for an immediately
//!   started request) -> optional bounded reports at real phase boundaries
//!   (queued -> analyzing, analyzing -> publishing) -> exactly one terminal
//!   end (complete | limited | failed | cancelled | superseded | not_started).
//!
//! Rules enforced here:
//!
//! - deduplicated requests never reach this module, so they create no token;
//! - the denominator for real percentages is unknown, so no percentage is
//!   ever emitted — title/message + phase only;
//! - a terminal end is sent exactly once per token: `end*` removes the token
//!   from the registry before notifying, so every later end for the same
//!   generation is a no-op;
//! - progress is best-effort: a client error on
//!   `window/workDoneProgress/create` is logged and the token is dropped
//!   (per the spec no progress notifications may follow a failed create);
//!   analysis state is never touched;
//! - clients without `window.workDoneProgress` support see no progress
//!   traffic at all (`supported` stays false and every entry point no-ops);
//! - generic messages only: no file paths, source excerpts, or config
//!   values. Reasons, scopes, failure kinds, and run statuses are internal
//!   enum tags.
//!
//! The richer RIPR-specific `ripr/analysisStatus` notification remains the
//! authoritative status channel; the terminal end message is derived from
//! the same attempt outcome/run status so the two surfaces agree.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tower_lsp_server::Client;
use tower_lsp_server::ls_types::{
    MessageType, ProgressParams, ProgressParamsValue, ProgressToken, WorkDoneProgress,
    WorkDoneProgressBegin, WorkDoneProgressEnd, WorkDoneProgressReport,
    notification::Progress as ProgressNotification,
};

use super::progress_stages::stage_report_message;
use super::refresh_scheduler::RefreshRequest;
use super::state::AnalysisFailureKind;

use crate::app::AnalysisProgressStage;

/// Lifecycle phase of an accepted analysis request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AnalysisProgressPhase {
    /// Accepted while another analysis is active; waiting to start.
    Queued,
    /// Actively running workspace analysis.
    Analyzing,
}

impl AnalysisProgressPhase {
    fn begin_message(self, request: &RefreshRequest) -> String {
        match self {
            Self::Queued => format!(
                "queued; waiting for the active analysis ({})",
                request.reason.as_str()
            ),
            Self::Analyzing => format!("analyzing workspace ({})", request.reason.as_str()),
        }
    }
}

/// Terminal state for one progress token. Every accepted generation ends
/// with exactly one of these on every path — completion, failure,
/// cancellation, deadline expiry, supersession, root invalidation, or a
/// pre-start stop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum AnalysisProgressEnd {
    /// Snapshot published with run status `full`.
    Complete,
    /// Snapshot published with a limited run status. Carries no run-status
    /// tag: the tag is machine-only vocabulary and stays on the typed
    /// `ripr/analysisStatus` payload; the client-facing end message is a
    /// pinned human phrase (see `Self::message`).
    Limited,
    /// Analysis or analysis task failed (carries the internal failure kind
    /// tag, e.g. `analysis_error`/`task_failure`; never the raw error, which
    /// may contain paths).
    Failed(Option<AnalysisFailureKind>),
    Cancelled,
    /// The physical refresh deadline (#1972) expired mid-attempt; the
    /// refresh was dropped fail-closed with no committed limited snapshot.
    DeadlineExceeded,
    Superseded,
    NotStarted,
}

impl AnalysisProgressEnd {
    fn message(&self) -> String {
        match self {
            Self::Complete => "analysis complete".to_string(),
            Self::Limited => "analysis completed with limited evidence".to_string(),
            Self::Failed(Some(kind)) => format!("analysis failed ({})", kind.as_str()),
            Self::Failed(None) => "analysis failed".to_string(),
            Self::Cancelled => "analysis cancelled".to_string(),
            Self::DeadlineExceeded => "analysis deadline exceeded".to_string(),
            Self::Superseded => "analysis superseded by a newer request".to_string(),
            Self::NotStarted => "analysis did not start".to_string(),
        }
    }
}

/// One `$/progress` notification payload.
pub(super) enum ProgressNotificationValue {
    Begin { title: String, message: String },
    Report { message: String },
    End { message: String },
}

pub(super) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Transport for progress traffic. Abstracted so unit tests can record the
/// wire sequence without a live client.
pub(super) trait ProgressSink: Send + Sync {
    fn create(&self, token: String) -> BoxFuture<'_, Result<(), String>>;
    fn notify(&self, token: String, value: ProgressNotificationValue) -> BoxFuture<'_, ()>;
}

impl ProgressSink for Client {
    fn create(&self, token: String) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            self.create_work_done_progress(ProgressToken::String(token))
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn notify(&self, token: String, value: ProgressNotificationValue) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let value = match value {
                ProgressNotificationValue::Begin { title, message } => {
                    ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(WorkDoneProgressBegin {
                        title,
                        // Server-driven analysis is not cancelled through this
                        // token, so no cancel button is advertised.
                        cancellable: Some(false),
                        message: Some(message),
                        // No fabricated percentages: the denominator is
                        // unknown, so progress stays unbounded.
                        percentage: None,
                    }))
                }
                ProgressNotificationValue::Report { message } => ProgressParamsValue::WorkDone(
                    WorkDoneProgress::Report(WorkDoneProgressReport {
                        message: Some(message),
                        ..WorkDoneProgressReport::default()
                    }),
                ),
                ProgressNotificationValue::End { message } => {
                    ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
                        message: Some(message),
                    }))
                }
            };
            self.send_notification::<ProgressNotification>(ProgressParams {
                token: ProgressToken::String(token),
                value,
            })
            .await;
        })
    }
}

struct TokenRecord {
    token: String,
    phase: AnalysisProgressPhase,
    // Shared with an in-flight sender even after terminal registry removal.
    notifications: Arc<tokio::sync::Mutex<TokenNotifications>>,
}

#[derive(Default)]
struct TokenNotifications {
    /// True once the `$/progress` begin notification has actually been sent.
    /// A token ended while `started == false` is removed silently: per the LSP
    /// spec no progress notifications may precede a begin, and none may follow
    /// a failed create.
    started: bool,
    /// Last producer stage forwarded as a report on this token (#4811).
    /// Consecutive duplicate stage events are suppressed so repeated
    /// equivalent events stay within the declared record ceiling.
    last_stage: Option<AnalysisProgressStage>,
}

fn progress_token(generation: u64) -> String {
    format!("ripr-analysis-{generation}")
}

/// Owns the progress-token registry for accepted analysis generations.
///
/// Exactly-once terminal semantics come from the registry: a token is only
/// ended after it has been removed, so concurrent or repeated terminal
/// paths (outcome end, cancellation guard, invalidation drain, shutdown
/// drain) cannot emit two ends for one token.
pub(super) struct AnalysisProgressTracker {
    client: Client,
    sink: Mutex<Arc<dyn ProgressSink>>,
    supported: AtomicBool,
    tokens: Mutex<BTreeMap<u64, TokenRecord>>,
    /// Liveness bound for the `window/workDoneProgress/create` round trip
    /// (#5278). Announcement is detached from refresh (#9149); the bound
    /// also limits how long that best-effort task waits for the client.
    client_request_timeout: std::time::Duration,
}

impl AnalysisProgressTracker {
    pub(super) fn new(client: Client) -> Self {
        Self::with_client_request_timeout(client, super::transport_bounds::CLIENT_REQUEST_TIMEOUT)
    }

    pub(super) fn with_client_request_timeout(
        client: Client,
        client_request_timeout: std::time::Duration,
    ) -> Self {
        let sink: Arc<dyn ProgressSink> = Arc::new(client.clone());
        Self {
            client,
            sink: Mutex::new(sink),
            supported: AtomicBool::new(false),
            tokens: Mutex::new(BTreeMap::new()),
            client_request_timeout,
        }
    }

    pub(super) fn set_supported(&self, supported: bool) {
        self.supported.store(supported, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn is_supported(&self) -> bool {
        self.supported.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    fn set_sink(&self, sink: Arc<dyn ProgressSink>) {
        if let Ok(mut current) = self.sink.lock() {
            *current = sink;
        }
    }

    #[cfg(test)]
    pub(super) fn install_recorder(&self, sink: Arc<RecordingSink>) {
        let recorder: Arc<dyn ProgressSink> = sink;
        self.set_sink(recorder);
        self.set_supported(true);
    }

    /// Install an arbitrary test transport (for example a sink whose sends
    /// never resolve) so bounded-drain guarantees can be exercised without a
    /// live client.
    #[cfg(test)]
    pub(super) fn install_test_sink(&self, sink: Arc<dyn ProgressSink>) {
        self.set_sink(sink);
        self.set_supported(true);
    }

    fn sink(&self) -> Option<Arc<dyn ProgressSink>> {
        self.sink.lock().ok().map(|sink| Arc::clone(&sink))
    }

    /// Register before spawning the announcement so termination can always
    /// remove an accepted token, even before the task is first polled.
    fn register(&self, request: &RefreshRequest, phase: AnalysisProgressPhase) -> bool {
        if !self.supported.load(Ordering::SeqCst) {
            return false;
        }
        let Ok(mut tokens) = self.tokens.lock() else {
            return false;
        };
        if tokens.contains_key(&request.generation) {
            return false;
        }
        tokens.insert(
            request.generation,
            TokenRecord {
                token: progress_token(request.generation),
                phase,
                notifications: Arc::new(tokio::sync::Mutex::new(TokenNotifications::default())),
            },
        );
        true
    }

    /// Progress is advisory: an unanswered create must not hold refresh or
    /// shutdown in the transport's request pipeline (#9149).
    pub(super) fn begin_detached(
        self: &Arc<Self>,
        request: &RefreshRequest,
        phase: AnalysisProgressPhase,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !self.register(request, phase) {
            return None;
        }
        let tracker = Arc::clone(self);
        let request = request.clone();
        Some(tokio::spawn(
            async move { tracker.announce(&request).await },
        ))
    }

    #[cfg(test)]
    pub(super) async fn begin(&self, request: &RefreshRequest, phase: AnalysisProgressPhase) {
        if self.register(request, phase) {
            self.announce(request).await;
        }
    }

    async fn announce(&self, request: &RefreshRequest) {
        let Some(notifications) = self.notification_state(request.generation) else {
            return;
        };
        let Some(sink) = self.sink() else {
            return;
        };
        let token = progress_token(request.generation);
        // #5278: the create is a server→client request whose response can
        // never arrive once the transport read loop has fused (or if the
        // client simply never answers). Expiry takes the same drop-and-log
        // path as any other create failure; analysis state is unaffected.
        let created =
            match tokio::time::timeout(self.client_request_timeout, sink.create(token.clone()))
                .await
            {
                Ok(result) => result,
                Err(_) => Err(format!(
                    "no response within its {}s liveness bound (#5278)",
                    self.client_request_timeout.as_secs()
                )),
            };
        if let Err(error) = created {
            if let Ok(mut tokens) = self.tokens.lock() {
                tokens.remove(&request.generation);
            }
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!(
                        "ripr work-done progress unavailable for generation {}: {error}",
                        request.generation
                    ),
                )
                .await;
            return;
        }
        let mut notifications = notifications.lock().await;
        let phase = {
            let Ok(tokens) = self.tokens.lock() else {
                return;
            };
            let Some(record) = tokens.get(&request.generation) else {
                // Terminated while create was pending: #1971 requires silence.
                return;
            };
            record.phase
        };
        sink.notify(
            token.clone(),
            ProgressNotificationValue::Begin {
                title: "ripr analysis".to_string(),
                message: phase.begin_message(request),
            },
        )
        .await;
        // A terminal owner may have removed the registry entry while Begin
        // was sending. Its retained state must still observe this completed
        // Begin and send exactly one End after acquiring the same token lock.
        notifications.started = true;
    }

    fn notification_state(
        &self,
        generation: u64,
    ) -> Option<Arc<tokio::sync::Mutex<TokenNotifications>>> {
        let tokens = self.tokens.lock().ok()?;
        tokens
            .get(&generation)
            .map(|record| Arc::clone(&record.notifications))
    }

    /// Move a queued token to `analyzing` on the SAME token when the queued
    /// request starts. An unstarted token records the new phase silently.
    /// No-op for tokens already analyzing or unknown.
    pub(super) async fn transition_to_analyzing(&self, generation: u64) {
        let Some(notifications) = self.notification_state(generation) else {
            return;
        };
        let notifications = notifications.lock().await;
        let token = {
            let Ok(mut tokens) = self.tokens.lock() else {
                return;
            };
            let Some(record) = tokens.get_mut(&generation) else {
                return;
            };
            if record.phase != AnalysisProgressPhase::Queued {
                return;
            }
            record.phase = AnalysisProgressPhase::Analyzing;
            if !notifications.started {
                // A delayed create must announce the current phase when it
                // succeeds, rather than claiming the promoted request is queued.
                return;
            }
            record.token.clone()
        };
        let Some(sink) = self.sink() else {
            return;
        };
        sink.notify(
            token,
            ProgressNotificationValue::Report {
                message: "analyzing workspace".to_string(),
            },
        )
        .await;
    }

    /// Bounded report at the real phase boundary where analysis results are
    /// about to be published.
    pub(super) async fn report_publishing(&self, generation: u64) {
        let Some(notifications) = self.notification_state(generation) else {
            return;
        };
        let notifications = notifications.lock().await;
        let token = {
            let Ok(tokens) = self.tokens.lock() else {
                return;
            };
            let Some(record) = tokens.get(&generation) else {
                return;
            };
            if !notifications.started || record.phase != AnalysisProgressPhase::Analyzing {
                return;
            }
            record.token.clone()
        };
        let Some(sink) = self.sink() else {
            return;
        };
        sink.notify(
            token,
            ProgressNotificationValue::Report {
                message: "publishing diagnostics".to_string(),
            },
        )
        .await;
    }

    /// Bounded report carrying one shared producer stage identity (#4811).
    ///
    /// This is the standard work-done consumption of the same
    /// `AnalysisProgressStage` vocabulary the CLI projects onto stderr:
    /// `loading_input`, `analyzing`, and `building_output` each become one
    /// client-appropriate bounded report on the accepted generation's token.
    /// Terminal stages are never reported here — the terminal disposition
    /// stays derived from the attempt outcome so cancellation, timeout,
    /// supersession, failure, and limited/deferred states keep their typed
    /// honesty and can never publish a fake completed stage.
    ///
    /// Reports are message-only: no percentage is ever attached, so unknown
    /// totals stay unknown. No-ops (never an error, never analysis-relevant):
    /// unsupported clients, unknown generations, unstarted tokens, tokens
    /// still queued, terminal stages, and consecutive duplicate stages.
    pub(super) async fn report_stage(&self, generation: u64, stage: AnalysisProgressStage) {
        let Some(notifications) = self.notification_state(generation) else {
            return;
        };
        let mut notifications = notifications.lock().await;
        if !self.supported.load(Ordering::SeqCst) {
            return;
        }
        let Some(message) = stage_report_message(stage) else {
            return;
        };
        let token = {
            let Ok(mut tokens) = self.tokens.lock() else {
                return;
            };
            let Some(record) = tokens.get_mut(&generation) else {
                return;
            };
            if !notifications.started || record.phase != AnalysisProgressPhase::Analyzing {
                return;
            }
            if notifications.last_stage == Some(stage) {
                return;
            }
            notifications.last_stage = Some(stage);
            record.token.clone()
        };
        let Some(sink) = self.sink() else {
            return;
        };
        sink.notify(
            token,
            ProgressNotificationValue::Report {
                message: message.to_string(),
            },
        )
        .await;
    }

    /// Terminate one generation's token. Exactly-once: the token is removed
    /// before the end notification, so later ends for the same generation
    /// are no-ops.
    pub(super) async fn end(&self, generation: u64, end: AnalysisProgressEnd) {
        let record = {
            let Ok(mut tokens) = self.tokens.lock() else {
                return;
            };
            tokens.remove(&generation)
        };
        if let Some(record) = record {
            self.end_record(record, &end).await;
        }
    }

    async fn end_record(&self, record: TokenRecord, end: &AnalysisProgressEnd) {
        // Only this token's sender can delay its End. Registry removal already
        // prevents late announcements/reports and gives this owner sole custody.
        let notifications = record.notifications.lock().await;
        if !notifications.started {
            // #1971: no Begin was sent, so termination remains silent.
            return;
        }
        let Some(sink) = self.sink() else {
            return;
        };
        sink.notify(
            record.token,
            ProgressNotificationValue::End {
                message: end.message(),
            },
        )
        .await;
    }

    /// Terminate every token still in `queued` phase. Used by root/config
    /// invalidation and cancellation paths where a queued request is dropped
    /// without ever starting; the active (analyzing) token is left for the
    /// refresh loop's outcome-based end.
    pub(super) async fn end_queued(&self, end: AnalysisProgressEnd) {
        let drained = {
            let Ok(mut tokens) = self.tokens.lock() else {
                return;
            };
            let queued: Vec<u64> = tokens
                .iter()
                .filter(|(_, record)| record.phase == AnalysisProgressPhase::Queued)
                .map(|(generation, _)| *generation)
                .collect();
            queued
                .into_iter()
                .filter_map(|generation| tokens.remove(&generation))
                .collect::<Vec<_>>()
        };
        for record in drained {
            self.end_record(record, &end).await;
        }
    }

    /// Terminate every open token. Used on shutdown when the scheduler
    /// stops accepting and running work. A drain still awaits each token's
    /// in-flight notification; per-token serialization is not a total shutdown
    /// latency bound.
    pub(super) async fn end_all(&self, end: AnalysisProgressEnd) {
        let drained = {
            let Ok(mut tokens) = self.tokens.lock() else {
                return;
            };
            std::mem::take(&mut *tokens)
                .into_values()
                .collect::<Vec<_>>()
        };
        for record in drained {
            self.end_record(record, &end).await;
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ProgressEvent {
    Create {
        token: String,
    },
    Begin {
        token: String,
        title: String,
        message: String,
    },
    Report {
        token: String,
        message: String,
    },
    End {
        token: String,
        message: String,
    },
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct RecordingSink {
    events: Mutex<Vec<ProgressEvent>>,
    fail_create: AtomicBool,
}

#[cfg(test)]
impl RecordingSink {
    pub(super) fn events(&self) -> Vec<ProgressEvent> {
        self.events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }

    pub(super) fn set_fail_create(&self, fail: bool) {
        self.fail_create.store(fail, Ordering::SeqCst);
    }

    fn push(&self, event: ProgressEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

#[cfg(test)]
impl ProgressSink for RecordingSink {
    fn create(&self, token: String) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            self.push(ProgressEvent::Create { token });
            if self.fail_create.load(Ordering::SeqCst) {
                return Err("client rejected work-done progress create".to_string());
            }
            Ok(())
        })
    }

    fn notify(&self, token: String, value: ProgressNotificationValue) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            match value {
                ProgressNotificationValue::Begin { title, message } => {
                    self.push(ProgressEvent::Begin {
                        token,
                        title,
                        message,
                    });
                }
                ProgressNotificationValue::Report { message } => {
                    self.push(ProgressEvent::Report { token, message });
                }
                ProgressNotificationValue::End { message } => {
                    self.push(ProgressEvent::End { token, message });
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::LspAnalysisConfig;
    use super::super::input_identity::LspAnalysisInputIdentity;
    use super::super::refresh_scheduler::{RefreshReason, RefreshScope};
    use super::*;
    use crate::analysis::cancellation::AnalysisCancellationToken;
    use std::path::PathBuf;
    use tower_lsp_server::jsonrpc::Result as LspResult;
    use tower_lsp_server::ls_types::{InitializeParams, InitializeResult};
    use tower_lsp_server::{LanguageServer, LspService};

    struct ClientOnly(Client);

    impl LanguageServer for ClientOnly {
        async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
            Ok(InitializeResult::default())
        }

        async fn shutdown(&self) -> LspResult<()> {
            Ok(())
        }
    }

    fn test_client() -> Client {
        let (service, _socket) = LspService::new(ClientOnly);
        service.inner().0.clone()
    }

    fn tracker_with_recorder() -> (Arc<AnalysisProgressTracker>, Arc<RecordingSink>) {
        let tracker = Arc::new(AnalysisProgressTracker::new(test_client()));
        let sink = Arc::new(RecordingSink::default());
        tracker.install_recorder(Arc::clone(&sink));
        (tracker, sink)
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
            git_inputs: crate::lsp::git_inputs::ResolvedGitInputs::resolve(
                &root,
                config.base_ref.as_deref(),
                None,
            ),
            root,
            config,
            workspace_revision: generation,
            scope: RefreshScope::Interactive,
            reason: RefreshReason::DidSave,
            cancellation: AnalysisCancellationToken::new(),
        }
    }

    fn runtime() -> Result<tokio::runtime::Runtime, String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| format!("failed to start test runtime: {err}"))
    }

    #[test]
    fn success_run_begins_analyzing_and_ends_complete_exactly_once() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            let request = test_request(1);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            tracker.report_publishing(1).await;
            tracker.end(1, AnalysisProgressEnd::Complete).await;
            tracker.end(1, AnalysisProgressEnd::Complete).await;

            let events = sink.events();
            let expected = vec![
                ProgressEvent::Create {
                    token: "ripr-analysis-1".to_string(),
                },
                ProgressEvent::Begin {
                    token: "ripr-analysis-1".to_string(),
                    title: "ripr analysis".to_string(),
                    message: "analyzing workspace (did_save)".to_string(),
                },
                ProgressEvent::Report {
                    token: "ripr-analysis-1".to_string(),
                    message: "publishing diagnostics".to_string(),
                },
                ProgressEvent::End {
                    token: "ripr-analysis-1".to_string(),
                    message: "analysis complete".to_string(),
                },
            ];
            if events != expected {
                return Err(format!("unexpected event sequence: {events:?}"));
            }
            Ok(())
        })
    }

    #[test]
    fn queued_request_transitions_to_analyzing_on_the_same_token() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            let request = test_request(2);
            tracker.begin(&request, AnalysisProgressPhase::Queued).await;
            tracker.transition_to_analyzing(2).await;
            tracker.end(2, AnalysisProgressEnd::Limited).await;

            let events = sink.events();
            let token = "ripr-analysis-2".to_string();
            let expected = vec![
                ProgressEvent::Create {
                    token: token.clone(),
                },
                ProgressEvent::Begin {
                    token: token.clone(),
                    title: "ripr analysis".to_string(),
                    message: "queued; waiting for the active analysis (did_save)".to_string(),
                },
                ProgressEvent::Report {
                    token: token.clone(),
                    message: "analyzing workspace".to_string(),
                },
                ProgressEvent::End {
                    token: token.clone(),
                    message: "analysis completed with limited evidence".to_string(),
                },
            ];
            if events != expected {
                return Err(format!("queued lifecycle drifted: {events:?}"));
            }
            Ok(())
        })
    }

    /// Real create/send suspension points, rather than manually inserting
    /// an unstarted record, expose late success and begin/end races.
    struct HeldProgressSink {
        recorder: Arc<RecordingSink>,
        hold_begin: bool,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    impl ProgressSink for HeldProgressSink {
        fn create(&self, token: String) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async move {
                self.recorder.create(token).await?;
                if !self.hold_begin {
                    self.entered.notify_one();
                    self.release.notified().await;
                }
                Ok(())
            })
        }

        fn notify(&self, token: String, value: ProgressNotificationValue) -> BoxFuture<'_, ()> {
            Box::pin(async move {
                if self.hold_begin && matches!(value, ProgressNotificationValue::Begin { .. }) {
                    self.entered.notify_one();
                    self.release.notified().await;
                }
                self.recorder.notify(token, value).await;
            })
        }
    }

    #[test]
    fn late_create_success_never_revives_terminated_tokens() -> Result<(), String> {
        runtime()?.block_on(async {
            for terminal in ["end", "end_queued", "end_all"] {
                let (tracker, recorder) = tracker_with_recorder();
                let sink = Arc::new(HeldProgressSink {
                    recorder: Arc::clone(&recorder),
                    hold_begin: false,
                    entered: tokio::sync::Notify::new(),
                    release: tokio::sync::Notify::new(),
                });
                tracker.install_test_sink(sink.clone());
                let announcing = Arc::clone(&tracker);
                let announce = tokio::spawn(async move {
                    announcing
                        .begin(&test_request(7), AnalysisProgressPhase::Queued)
                        .await;
                });
                sink.entered.notified().await;
                match terminal {
                    "end" => tracker.end(7, AnalysisProgressEnd::Cancelled).await,
                    "end_queued" => tracker.end_queued(AnalysisProgressEnd::Superseded).await,
                    _ => tracker.end_all(AnalysisProgressEnd::Cancelled).await,
                }
                sink.release.notify_one();
                announce
                    .await
                    .map_err(|error| format!("announce task failed: {error}"))?;
                if recorder.events()
                    != [ProgressEvent::Create {
                        token: progress_token(7),
                    }]
                {
                    return Err(format!(
                        "{terminal}: late create revived terminated token: {:?}",
                        recorder.events()
                    ));
                }
            }
            Ok(())
        })
    }

    #[test]
    fn termination_during_begin_send_ends_the_started_token_exactly_once() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, recorder) = tracker_with_recorder();
            let sink = Arc::new(HeldProgressSink {
                recorder: Arc::clone(&recorder),
                hold_begin: true,
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            });
            tracker.install_test_sink(sink.clone());
            let announcing = Arc::clone(&tracker);
            let announce = tokio::spawn(async move {
                announcing
                    .begin(&test_request(8), AnalysisProgressPhase::Analyzing)
                    .await;
            });
            sink.entered.notified().await;
            let ending = Arc::clone(&tracker);
            let end = tokio::spawn(async move {
                ending.end_all(AnalysisProgressEnd::Cancelled).await;
                ending.end(8, AnalysisProgressEnd::Cancelled).await;
            });
            // Poll the competing termination while begin is still held.
            tokio::task::yield_now().await;
            sink.release.notify_one();
            announce
                .await
                .map_err(|error| format!("announce task failed: {error}"))?;
            end.await
                .map_err(|error| format!("end task failed: {error}"))?;
            let events = recorder.events();
            if !matches!(events.as_slice(), [
                ProgressEvent::Create { token: create },
                ProgressEvent::Begin { token: begin, .. },
                ProgressEvent::End { token: end, .. },
            ] if create == &progress_token(8) && begin == create && end == create)
            {
                return Err(format!(
                    "begin/end must stay ordered and end exactly once: {events:?}"
                ));
            }
            Ok(())
        })
    }

    #[test]
    fn detached_announcement_registers_before_its_task_is_polled() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, recorder) = tracker_with_recorder();
            let announcement = tracker
                .begin_detached(&test_request(12), AnalysisProgressPhase::Analyzing)
                .ok_or_else(|| "supported generation was not registered".to_string())?;
            // No yield: shutdown wins before the announcement task runs.
            tracker.end_all(AnalysisProgressEnd::Cancelled).await;
            announcement
                .await
                .map_err(|error| format!("announcement failed: {error}"))?;
            if !recorder.events().is_empty() {
                return Err(format!(
                    "terminated registration was announced: {:?}",
                    recorder.events()
                ));
            }
            Ok(())
        })
    }

    #[test]
    fn queued_promotion_during_create_announces_the_current_phase() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, recorder) = tracker_with_recorder();
            let sink = Arc::new(HeldProgressSink {
                recorder: Arc::clone(&recorder),
                hold_begin: false,
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            });
            tracker.install_test_sink(sink.clone());
            let announcement = tracker
                .begin_detached(&test_request(13), AnalysisProgressPhase::Queued)
                .ok_or_else(|| "supported queued generation was not registered".to_string())?;
            sink.entered.notified().await;
            tracker.transition_to_analyzing(13).await;
            sink.release.notify_one();
            announcement
                .await
                .map_err(|error| format!("announcement failed: {error}"))?;
            tracker.end(13, AnalysisProgressEnd::Complete).await;
            tracker.end_all(AnalysisProgressEnd::Cancelled).await;
            let events = recorder.events();
            if !matches!(events.as_slice(), [
                ProgressEvent::Create { token: create },
                ProgressEvent::Begin { token: begin, message, .. },
                ProgressEvent::End { token: end, .. },
            ] if create == &progress_token(13) && begin == create && end == create
                && message == "analyzing workspace (did_save)")
            {
                return Err(format!(
                    "delayed announcement used a stale phase or lost its end: {events:?}"
                ));
            }
            Ok(())
        })
    }

    #[test]
    fn capability_absent_client_sees_no_progress_traffic() -> Result<(), String> {
        runtime()?.block_on(async {
            let (service, _socket) = LspService::new(ClientOnly);
            let tracker = AnalysisProgressTracker::new(service.inner().0.clone());
            let sink = Arc::new(RecordingSink::default());
            tracker.set_sink(sink.clone());
            // `supported` stays false: the client never advertised
            // window.workDoneProgress.
            let request = test_request(1);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            tracker.transition_to_analyzing(1).await;
            tracker.report_publishing(1).await;
            tracker.end(1, AnalysisProgressEnd::Complete).await;
            tracker.end_queued(AnalysisProgressEnd::Cancelled).await;
            tracker.end_all(AnalysisProgressEnd::Cancelled).await;

            if !sink.events().is_empty() {
                return Err(format!(
                    "capability-absent client received progress traffic: {:?}",
                    sink.events()
                ));
            }
            if tracker.is_supported() {
                return Err("tracker must stay unsupported without the client capability".into());
            }
            Ok(())
        })
    }

    #[test]
    fn end_before_begin_notification_removes_token_silently() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            // Simulate the mid-create window: a registered but unstarted token.
            {
                let Ok(mut tokens) = tracker.tokens.lock() else {
                    return Err("tracker lock poisoned".to_string());
                };
                tokens.insert(
                    7,
                    TokenRecord {
                        token: progress_token(7),
                        phase: AnalysisProgressPhase::Queued,
                        notifications: Arc::new(tokio::sync::Mutex::new(
                            TokenNotifications::default(),
                        )),
                    },
                );
            }
            tracker.end(7, AnalysisProgressEnd::Cancelled).await;
            if !sink.events().is_empty() {
                return Err(format!(
                    "an unstarted token emitted progress traffic: {:?}",
                    sink.events()
                ));
            }
            let Ok(tokens) = tracker.tokens.lock() else {
                return Err("tracker lock poisoned".to_string());
            };
            if tokens.contains_key(&7) {
                return Err("unstarted token was not removed by end".to_string());
            }
            Ok(())
        })
    }

    /// #5278: a create that never resolves (a client that never answers
    /// `window/workDoneProgress/create`) must expire through the same
    /// drop-and-log path as a failed create instead of pinning the refresh
    /// path — and with it the transport's task pipeline — forever.
    #[test]
    fn unanswered_progress_create_expires_and_drops_the_token() -> Result<(), String> {
        struct PendingCreateSink;
        impl ProgressSink for PendingCreateSink {
            fn create(&self, _token: String) -> BoxFuture<'_, Result<(), String>> {
                Box::pin(std::future::pending())
            }
            fn notify(
                &self,
                _token: String,
                _value: ProgressNotificationValue,
            ) -> BoxFuture<'_, ()> {
                Box::pin(async {})
            }
        }
        runtime()?.block_on(async {
            let tracker = Arc::new(AnalysisProgressTracker::with_client_request_timeout(
                test_client(),
                std::time::Duration::from_millis(100),
            ));
            tracker.set_supported(true);
            tracker.set_sink(Arc::new(PendingCreateSink));
            let request = test_request(11);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            {
                let Ok(tokens) = tracker.tokens.lock() else {
                    return Err("tracker lock poisoned".to_string());
                };
                if tokens.contains_key(&11) {
                    return Err(
                        "an expired create left a placeholder token that would leak or block retry"
                            .to_string(),
                    );
                }
            }
            // The generation recovers once the client answers creates again.
            tracker.set_sink(Arc::new(RecordingSink::default()));
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            {
                let Ok(tokens) = tracker.tokens.lock() else {
                    return Err("tracker lock poisoned".to_string());
                };
                if !tokens.contains_key(&11) {
                    return Err("a healthy retry must register the token".to_string());
                }
            }
            Ok(())
        })
    }

    #[test]
    fn create_failure_drops_placeholder_and_allows_retry() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            sink.set_fail_create(true);
            let request = test_request(9);
            tracker.begin(&request, AnalysisProgressPhase::Analyzing).await;
            {
                let Ok(tokens) = tracker.tokens.lock() else {
                    return Err("tracker lock poisoned".to_string());
                };
                if tokens.contains_key(&9) {
                    return Err(
                        "failed create left a placeholder token that would leak or block retry"
                            .to_string(),
                    );
                }
            }
            sink.set_fail_create(false);
            tracker.begin(&request, AnalysisProgressPhase::Analyzing).await;
            let events = sink.events();
            let begins = events
                .iter()
                .filter(|event| matches!(event, ProgressEvent::Begin { token, .. } if token == &progress_token(9)))
                .count();
            if begins != 1 {
                return Err(format!(
                    "retry after create failure did not begin exactly once: {events:?}"
                ));
            }
            Ok(())
        })
    }

    #[test]
    fn create_failure_is_best_effort_and_never_sends_notifications() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            sink.set_fail_create(true);
            let request = test_request(1);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            // The failed create drops the token: every later lifecycle call
            // for the generation is a no-op and cannot corrupt analysis.
            tracker.transition_to_analyzing(1).await;
            tracker.report_publishing(1).await;
            tracker.end(1, AnalysisProgressEnd::Complete).await;

            let events = sink.events();
            if events
                != vec![ProgressEvent::Create {
                    token: "ripr-analysis-1".to_string(),
                }]
            {
                return Err(format!(
                    "failed create must not emit progress notifications: {events:?}"
                ));
            }

            // A later generation recovers once the client accepts creates.
            sink.set_fail_create(false);
            let request = test_request(2);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            tracker.end(2, AnalysisProgressEnd::Complete).await;
            let events = sink.events();
            if !events
                .iter()
                .any(|event| matches!(event, ProgressEvent::Begin { token, .. } if token == "ripr-analysis-2"))
            {
                return Err(format!("recovered create did not begin: {events:?}"));
            }
            Ok(())
        })
    }

    #[test]
    fn end_queued_only_ends_queued_tokens_and_end_all_ends_everything() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            tracker
                .begin(&test_request(1), AnalysisProgressPhase::Analyzing)
                .await;
            tracker
                .begin(&test_request(2), AnalysisProgressPhase::Queued)
                .await;

            tracker.end_queued(AnalysisProgressEnd::Superseded).await;
            let events = sink.events();
            let ends: Vec<&ProgressEvent> = events
                .iter()
                .filter(|event| matches!(event, ProgressEvent::End { .. }))
                .collect();
            if ends.len() != 1 {
                return Err(format!(
                    "end_queued must end exactly the queued token: {events:?}"
                ));
            }
            if !matches!(
                ends.first(),
                Some(ProgressEvent::End { token, message })
                    if token == "ripr-analysis-2" && message.contains("superseded")
            ) {
                return Err(format!("wrong queued end: {events:?}"));
            }

            tracker.end_all(AnalysisProgressEnd::Cancelled).await;
            let events = sink.events();
            let end_count = events
                .iter()
                .filter(|event| matches!(event, ProgressEvent::End { .. }))
                .count();
            if end_count != 2 {
                return Err(format!("end_all must end the remaining token: {events:?}"));
            }
            // Everything is drained: further drains emit nothing.
            tracker.end_all(AnalysisProgressEnd::Cancelled).await;
            if sink.events().len() != events.len() {
                return Err("drained registry must stay drained".to_string());
            }
            Ok(())
        })
    }

    #[test]
    fn terminal_messages_carry_no_paths_or_source_excerpts() -> Result<(), String> {
        let ends = vec![
            AnalysisProgressEnd::Complete,
            AnalysisProgressEnd::Limited,
            AnalysisProgressEnd::Failed(Some(AnalysisFailureKind::TaskFailure)),
            AnalysisProgressEnd::Failed(None),
            AnalysisProgressEnd::Cancelled,
            AnalysisProgressEnd::Superseded,
            AnalysisProgressEnd::NotStarted,
        ];
        for end in ends {
            let message = end.message();
            if message.contains('/') || message.contains('\\') || message.contains(".rs") {
                return Err(format!("terminal message leaks path content: {message}"));
            }
        }
        Ok(())
    }

    #[test]
    fn limited_end_message_is_pinned_human_phrase_without_run_status_tag() -> Result<(), String> {
        // #5003: the raw run-status tag (`seams_deferred`, `cache_limited`,
        // ...) is machine-only vocabulary. The terminal work-done end message
        // must stay a human phrase; the tag remains available on the typed
        // `ripr/analysisStatus` payload.
        let message = AnalysisProgressEnd::Limited.message();
        if message != "analysis completed with limited evidence" {
            return Err(format!("limited end message drifted: {message:?}"));
        }
        let leaked_tags = [
            "run status",
            "seams_deferred",
            "cache_limited",
            "limited_partial_scope",
        ];
        for leaked in leaked_tags {
            if message.contains(leaked) {
                return Err(format!(
                    "limited end message leaks the internal run-status tag {leaked:?}: {message:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn reports_and_ends_for_unknown_generations_are_no_ops() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            tracker.transition_to_analyzing(99).await;
            tracker.report_publishing(99).await;
            tracker.end(99, AnalysisProgressEnd::Complete).await;
            if !sink.events().is_empty() {
                return Err(format!(
                    "unknown generations must not emit traffic: {:?}",
                    sink.events()
                ));
            }
            Ok(())
        })
    }

    #[test]
    fn report_stage_emits_one_bounded_report_per_stage_in_producer_order() -> Result<(), String> {
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            let request = test_request(1);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            for stage in [
                AnalysisProgressStage::LoadingInput,
                AnalysisProgressStage::Analyzing,
                AnalysisProgressStage::BuildingOutput,
            ] {
                tracker.report_stage(1, stage).await;
            }
            tracker.end(1, AnalysisProgressEnd::Complete).await;

            let events = sink.events();
            let token = "ripr-analysis-1".to_string();
            let expected = vec![
                ProgressEvent::Create {
                    token: token.clone(),
                },
                ProgressEvent::Begin {
                    token: token.clone(),
                    title: "ripr analysis".to_string(),
                    message: "analyzing workspace (did_save)".to_string(),
                },
                ProgressEvent::Report {
                    token: token.clone(),
                    message: "loading input".to_string(),
                },
                ProgressEvent::Report {
                    token: token.clone(),
                    message: "analyzing workspace".to_string(),
                },
                ProgressEvent::Report {
                    token: token.clone(),
                    message: "building output".to_string(),
                },
                ProgressEvent::End {
                    token: token.clone(),
                    message: "analysis complete".to_string(),
                },
            ];
            if events != expected {
                return Err(format!("stage-mapped reports drifted: {events:?}"));
            }
            Ok(())
        })
    }

    #[test]
    fn report_stage_suppresses_consecutive_duplicate_stages() -> Result<(), String> {
        // Control 7: repeated equivalent stage events stay within the
        // declared record ceiling — one report per distinct stage.
        runtime()?.block_on(async {
            let (tracker, sink) = tracker_with_recorder();
            let request = test_request(1);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            for stage in [
                AnalysisProgressStage::LoadingInput,
                AnalysisProgressStage::LoadingInput,
                AnalysisProgressStage::Analyzing,
                AnalysisProgressStage::Analyzing,
            ] {
                tracker.report_stage(1, stage).await;
            }
            tracker.end(1, AnalysisProgressEnd::Complete).await;

            let reports = sink
                .events()
                .iter()
                .filter(|event| matches!(event, ProgressEvent::Report { .. }))
                .count();
            if reports != 2 {
                return Err(format!(
                    "duplicate stage events must collapse to one report each: {:?}",
                    sink.events()
                ));
            }
            Ok(())
        })
    }

    #[test]
    fn report_stage_is_silent_for_terminals_queued_unstarted_and_unknown() -> Result<(), String> {
        runtime()?.block_on(async {
            // Terminal stages never become reports: the outcome-derived end
            // owns the terminal disposition.
            let (tracker, sink) = tracker_with_recorder();
            let request = test_request(1);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            for stage in [
                AnalysisProgressStage::Completed,
                AnalysisProgressStage::Cancelled,
                AnalysisProgressStage::Failed,
            ] {
                tracker.report_stage(1, stage).await;
            }
            let mut reports = sink
                .events()
                .iter()
                .filter(|event| matches!(event, ProgressEvent::Report { .. }))
                .count();
            if reports != 0 {
                return Err(format!(
                    "terminal stages must never be reported: {:?}",
                    sink.events()
                ));
            }

            // Unknown generation is a no-op.
            tracker
                .report_stage(99, AnalysisProgressStage::LoadingInput)
                .await;

            // A queued (started) token must not carry stage reports: only
            // the active analyzing attempt may.
            let queued = test_request(2);
            tracker.begin(&queued, AnalysisProgressPhase::Queued).await;
            tracker
                .report_stage(2, AnalysisProgressStage::LoadingInput)
                .await;

            reports = sink
                .events()
                .iter()
                .filter(|event| matches!(event, ProgressEvent::Report { .. }))
                .count();
            if reports != 0 {
                return Err(format!(
                    "queued/unknown generations must not receive stage reports: {:?}",
                    sink.events()
                ));
            }
            Ok(())
        })
    }

    #[test]
    fn report_stage_is_silent_without_client_capability() -> Result<(), String> {
        runtime()?.block_on(async {
            let (service, _socket) = LspService::new(ClientOnly);
            let tracker = AnalysisProgressTracker::new(service.inner().0.clone());
            let sink = Arc::new(RecordingSink::default());
            tracker.set_sink(sink.clone());
            // `supported` stays false: no stage traffic may reach the client.
            let request = test_request(1);
            tracker
                .begin(&request, AnalysisProgressPhase::Analyzing)
                .await;
            tracker
                .report_stage(1, AnalysisProgressStage::LoadingInput)
                .await;
            tracker
                .report_stage(1, AnalysisProgressStage::BuildingOutput)
                .await;
            if !sink.events().is_empty() {
                return Err(format!(
                    "capability-absent client received stage reports: {:?}",
                    sink.events()
                ));
            }
            Ok(())
        })
    }
}
