use super::{gaps, protocol, repair, repair_card, workspace};
use crate::workspace_status::WorkspaceStatus;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::*,
    service::{NotificationContext, RequestContext},
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Application adapter only. The SDK owns RPC dispatch and lifecycle, except
/// initialize-session `ping`: method name wins over `params._meta` so a
/// handshake-shaped keepalive is not classified as a 2026-07-28 request
/// (#6022).
pub(super) struct McpServer {
    tools: ListToolsResult,
    resources: ListResourcesResult,
    resource_templates: ListResourceTemplatesResult,
    status: WorkspaceStatus,
    profile: workspace::SessionProfile,
    analysis_root: Option<PathBuf>,
    root_identity: Option<String>,
    session: Arc<Mutex<workspace::WorkspaceSession>>,
}

impl McpServer {
    pub(super) fn new(
        status: WorkspaceStatus,
        analysis_root: Option<PathBuf>,
    ) -> Result<Self, ErrorData> {
        // The session profile resolves the workspace's own configuration
        // once at startup (#6825): the same `load_for_root` posture the
        // refresh attempt runs under, so `ripr_workspace_status` never
        // discloses a language gate the analysis does not have.
        let profile = workspace::SessionProfile::resolve(analysis_root.as_deref());
        let session = workspace::WorkspaceSession {
            workspace_failure: workspace::available_root(analysis_root.as_deref()).err(),
            ..Default::default()
        };
        let mut server = Self {
            tools: typed(protocol::tools_list_result())?,
            resources: typed(protocol::resources_list_result())?,
            resource_templates: typed(protocol::resource_templates_list_result())?,
            profile,
            root_identity: status.root.identity.clone(),
            analysis_root,
            status,
            session: Arc::new(Mutex::new(session)),
        };
        // Domain JSON carries status/catalog data, not version-dependent SDK
        // result metadata. SDK constructors supply current result fields and
        // its handler owns stripping them for older peers.
        server.tools = ListToolsResult::with_all_items(server.tools.tools)
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private);
        server.resources = ListResourcesResult::with_all_items(server.resources.resources)
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private);
        server.resource_templates = ListResourceTemplatesResult::with_all_items(
            server.resource_templates.resource_templates,
        )
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private);
        Ok(server)
    }

    async fn status_tool(&self, max_envelope_bytes: usize) -> Result<CallToolResponse, ErrorData> {
        let session = self.session.lock().await;
        let envelope = match protocol::status_tool_result(
            &self.status,
            &session,
            &self.profile,
            super::MAX_MESSAGE_BYTES,
            super::MAX_RESPONSE_BYTES,
            max_envelope_bytes,
        ) {
            Ok(envelope) => envelope,
            // The envelope builder fails closed when the structured
            // envelope cannot fit the bound (#6021, #7768): that is the
            // typed `result_too_large` state, not an instrument error, and
            // no text-only success replaces the structured copy.
            Err(detail) => {
                let failure = workspace::AttemptFailure::new(
                    workspace::CODE_RESULT_TOO_LARGE,
                    format!("serialize workspace status: {detail}"),
                    workspace::RECOVERY_STATUS_TOO_LARGE,
                );
                return self.typed_failure(failure, workspace::SESSION_SCHEMA_VERSION);
            }
        };
        // Status goes through the same bound as every other tool (#5254
        // item 4): only the writer backstop guarded it before.
        self.bounded_tool_envelope(
            envelope,
            workspace::SESSION_SCHEMA_VERSION,
            "serialize workspace status",
            workspace::RECOVERY_STATUS_TOO_LARGE,
            max_envelope_bytes,
        )
    }

    async fn refresh_tool(&self, max_envelope_bytes: usize) -> Result<CallToolResponse, ErrorData> {
        let root = match workspace::available_root(self.analysis_root.as_deref()) {
            Ok(root) => root.to_path_buf(),
            Err(failure) => {
                return self.typed_failure(failure, workspace::REFRESH_SCHEMA_VERSION);
            }
        };
        let root_identity = self.root_identity.clone();
        let session = self.session.clone();
        {
            let mut session = session.lock().await;
            if session.in_flight {
                let failure = workspace::AttemptFailure::new(
                    workspace::CODE_ANALYSIS_IN_FLIGHT,
                    "an analysis attempt is already running",
                    "poll ripr_workspace_status until attempt_state leaves in_flight",
                );
                return self.typed_failure(failure, workspace::REFRESH_SCHEMA_VERSION);
            }
            session.in_flight = true;
        }
        let mut attempt = workspace::InFlightAttempt::new(session.clone());
        let worker = tokio::task::spawn_blocking(move || {
            workspace::run_check(&root, root_identity.as_deref())
        });
        let outcome = match worker.await {
            Ok(outcome) => outcome,
            Err(_join_error) => Err(workspace::AttemptFailure::new(
                workspace::CODE_ANALYSIS_FAILED,
                "the analysis worker terminated before producing a snapshot",
                "retry with ripr_refresh",
            )),
        };
        let document = {
            let mut session = session.lock().await;
            session.in_flight = false;
            attempt.disarm();
            match outcome {
                Ok(snapshot) => {
                    session.last_failure = None;
                    session.last_good = Some(Arc::new(snapshot));
                }
                Err(failure) => {
                    // A failed attempt never replaces the last-known-good
                    // snapshot and never commits a partial one.
                    session.last_failure = Some(failure);
                }
            }
            workspace::refresh_document(&session)
        };
        self.bounded_tool_result(
            document,
            workspace::REFRESH_SCHEMA_VERSION,
            "serialize refresh result",
            workspace::RECOVERY_REFRESH_TOO_LARGE,
            max_envelope_bytes,
        )
    }

    async fn list_gaps_tool(
        &self,
        arguments: Option<serde_json::Map<String, Value>>,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        reject_unknown_arguments(
            &arguments,
            &[
                "snapshot_id",
                "offset",
                "limit",
                "file",
                "class",
                "language",
                "repair_ready",
            ],
        )?;
        let requested = optional_string_argument(&arguments, "snapshot_id")?;
        let offset = optional_nonnegative_argument(&arguments, "offset")?;
        let limit = optional_positive_argument(&arguments, "limit")?;
        let file = optional_nonempty_string_argument(&arguments, "file")?;
        let class = optional_nonempty_string_argument(&arguments, "class")?;
        let language = optional_nonempty_string_argument(&arguments, "language")?;
        if let Some(language) = language.as_deref()
            && crate::domain::LanguageId::display_name_for_wire(language).is_none()
        {
            // A typo would otherwise read as "no gaps in that language".
            let allowed: Vec<&str> = crate::domain::LanguageId::ALL
                .iter()
                .map(crate::domain::LanguageId::as_str)
                .collect();
            // Echo only a short value: a huge one would push the error past
            // the response cap and replace -32602 with a generic -32603.
            let shown: String = language.chars().take(32).collect();
            return Err(ErrorData::invalid_params(
                format!("unknown language {shown:?}; allowed: {allowed:?}"),
                Some(serde_json::json!({ "allowed": allowed })),
            ));
        }
        let repair_ready = optional_bool_argument(&arguments, "repair_ready")?;
        let session = self.session.lock().await;
        match session.list_gaps(
            requested.as_deref(),
            workspace::GapListWindow {
                offset: offset.unwrap_or(0),
                limit,
            },
            &workspace::GapListFilter {
                file,
                class,
                language,
                repair_ready,
            },
            max_envelope_bytes,
        ) {
            Ok(document) => self.bounded_tool_result(
                document,
                gaps::GAP_LIST_SCHEMA_VERSION,
                "serialize gap list",
                workspace::RECOVERY_GAP_LIST_TOO_LARGE,
                max_envelope_bytes,
            ),
            Err(failure) => self.typed_failure(failure, gaps::GAP_LIST_SCHEMA_VERSION),
        }
    }

    async fn get_gap_tool(
        &self,
        arguments: Option<serde_json::Map<String, Value>>,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        reject_unknown_arguments(&arguments, &["canonical_id", "snapshot_id"])?;
        let canonical_id = required_string_argument(&arguments, "canonical_id")?;
        let requested = optional_string_argument(&arguments, "snapshot_id")?;
        let session = self.session.lock().await;
        match session.get_gap(&canonical_id, requested.as_deref()) {
            Ok(document) => self.bounded_tool_result(
                document,
                gaps::GAP_SCHEMA_VERSION,
                "serialize gap evidence",
                workspace::RECOVERY_GAP_ITEM_TOO_LARGE,
                max_envelope_bytes,
            ),
            Err(failure) => self.typed_failure(failure, gaps::GAP_SCHEMA_VERSION),
        }
    }

    async fn prepare_repair_tool(
        &self,
        arguments: Option<serde_json::Map<String, Value>>,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        reject_unknown_arguments(&arguments, &["canonical_id", "snapshot_id"])?;
        let canonical_id = required_string_argument(&arguments, "canonical_id")?;
        let requested = optional_string_argument(&arguments, "snapshot_id")?;
        let mut session = self.session.lock().await;
        match session.prepare_repair(
            &canonical_id,
            requested.as_deref(),
            self.root_identity.as_deref(),
        ) {
            Ok(document) => self.bounded_tool_result(
                document,
                repair::REPAIR_PACKET_SCHEMA_VERSION,
                "serialize repair packet",
                workspace::RECOVERY_REPAIR_TOO_LARGE,
                max_envelope_bytes,
            ),
            Err(failure) => self.typed_failure(failure, repair::REPAIR_PACKET_SCHEMA_VERSION),
        }
    }

    async fn list_repair_attempts_tool(
        &self,
        arguments: Option<serde_json::Map<String, Value>>,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        reject_unknown_arguments(&arguments, &["inventory_id", "offset", "limit"])?;
        let requested = optional_string_argument(&arguments, "inventory_id")?;
        let offset = optional_nonnegative_argument(&arguments, "offset")?.unwrap_or(0);
        let limit = optional_capped_page_limit(
            &arguments,
            "limit",
            super::attempt_inventory::MAX_PAGE_ITEMS,
        )?;
        // `max_envelope_bytes` reserves the JSON-escaped request ID plus the
        // fixed outer overhead (#7768): the pinned rmcp adds the JSON-RPC
        // fields and the modern resultType after the tool envelope is
        // measured, so near-cap pages keep structuredContent.
        let root = self.analysis_root.clone();
        let root_identity = self.root_identity.clone();
        let result = blocking_repair_read(move || {
            super::attempt_inventory::list_document(
                root.as_deref(),
                root_identity.as_deref(),
                requested.as_deref(),
                offset,
                limit,
                max_envelope_bytes,
            )
        })
        .await;
        match result {
            Ok(document) => self.bounded_tool_result(
                document,
                super::attempt_inventory::SCHEMA_VERSION,
                "serialize durable attempt inventory",
                super::attempt_inventory::RECOVERY_TOO_LARGE,
                max_envelope_bytes,
            ),
            Err(failure) => self.typed_failure(failure, super::attempt_inventory::SCHEMA_VERSION),
        }
    }

    async fn get_repair_attempt_tool(
        &self,
        arguments: Option<serde_json::Map<String, Value>>,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        reject_unknown_arguments(&arguments, &["attempt_id"])?;
        let attempt_id = required_string_argument(&arguments, "attempt_id")?;
        match self.repair_read_document(&attempt_id, false).await {
            Ok(document) => self.bounded_tool_result(
                document,
                repair::REPAIR_ATTEMPT_SCHEMA_VERSION,
                "serialize repair attempt",
                workspace::RECOVERY_REPAIR_TOO_LARGE,
                max_envelope_bytes,
            ),
            Err(failure) => self.typed_failure(failure, repair::REPAIR_ATTEMPT_SCHEMA_VERSION),
        }
    }

    async fn get_receipt_status_tool(
        &self,
        arguments: Option<serde_json::Map<String, Value>>,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        reject_unknown_arguments(&arguments, &["receipt_id"])?;
        let receipt_id = required_string_argument(&arguments, "receipt_id")?;
        match self.repair_read_document(&receipt_id, true).await {
            Ok(document) => self.bounded_tool_result(
                document,
                repair::RECEIPT_STATUS_SCHEMA_VERSION,
                "serialize receipt status",
                workspace::RECOVERY_REPAIR_TOO_LARGE,
                max_envelope_bytes,
            ),
            Err(failure) => self.typed_failure(failure, repair::RECEIPT_STATUS_SCHEMA_VERSION),
        }
    }

    /// Resolve session identities under the lock; only the independent durable
    /// fallback can perform Git/filesystem reads on the blocking worker. A
    /// session clone evaluated later could mistake a superseded snapshot for
    /// the current one, so session transactions never take that path.
    /// Public requests use BoundedTransport's admission gate through reply
    /// flush; a queued refresh cannot enter while this read is outstanding.
    async fn repair_read_document(
        &self,
        id: &str,
        receipt: bool,
    ) -> Result<Value, workspace::AttemptFailure> {
        let root = self.analysis_root.clone();
        let root_identity = self.root_identity.clone();
        let durable_id = id.to_string();
        self.repair_read_document_with(id, receipt, move || {
            repair_document_from_session(
                &workspace::WorkspaceSession::default(),
                &durable_id,
                receipt,
                root.as_deref(),
                root_identity.as_deref(),
            )
        })
        .await
    }

    async fn repair_read_document_with(
        &self,
        id: &str,
        receipt: bool,
        durable_read: impl FnOnce() -> Result<Value, workspace::AttemptFailure> + Send + 'static,
    ) -> Result<Value, workspace::AttemptFailure> {
        {
            let session = self.session.lock().await;
            if session.in_flight
                || session.repairs.contains_key(id)
                || session.superseded_attempts.contains_key(id)
                || self.analysis_root.is_none()
            {
                return repair_document_from_session(
                    &session,
                    id,
                    receipt,
                    self.analysis_root.as_deref(),
                    self.root_identity.as_deref(),
                );
            }
        }
        blocking_repair_read(durable_read).await
    }

    async fn get_repair_card_tool(
        &self,
        arguments: Option<serde_json::Map<String, Value>>,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        reject_unknown_arguments(&arguments, &["canonical_id", "snapshot_id"])?;
        let canonical_id = required_string_argument(&arguments, "canonical_id")?;
        let requested = optional_string_argument(&arguments, "snapshot_id")?;
        let session = self.session.lock().await;
        match session.repair_card_document(
            &canonical_id,
            requested.as_deref(),
            self.analysis_root.as_deref(),
        ) {
            Ok(document) => self.bounded_tool_result(
                document,
                repair_card::REPAIR_CARD_SCHEMA_VERSION,
                "serialize repair card",
                workspace::RECOVERY_REPAIR_TOO_LARGE,
                max_envelope_bytes,
            ),
            Err(failure) => self.typed_failure(failure, repair_card::REPAIR_CARD_SCHEMA_VERSION),
        }
    }

    fn typed_failure(
        &self,
        failure: workspace::AttemptFailure,
        schema_version: &str,
    ) -> Result<CallToolResponse, ErrorData> {
        let result = protocol::tool_failure(&failure, schema_version)
            .map_err(|_error| ErrorData::internal_error("serialize typed failure", None))?;
        let mut result: CallToolResult = typed(result)?;
        result.result_type = Some(ResultType::COMPLETE);
        Ok(result.into())
    }

    /// Wrap a document in the tool envelope. The envelope builder measures
    /// the final form against the caller's id-aware response budget before
    /// it ships (#6021, #7768): an envelope over that budget fails closed
    /// with the typed `result_too_large` failure here — every tool
    /// advertises an `outputSchema`, so a text-only success without
    /// `structuredContent` is not a shippable outcome — and only a document
    /// whose complete envelope fits ships.
    fn bounded_tool_result(
        &self,
        document: Value,
        failure_version: &'static str,
        serialize_context: &'static str,
        recovery: &'static str,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        let envelope = match protocol::tool_result(document, max_envelope_bytes) {
            Ok(envelope) => envelope,
            Err(detail) => {
                let failure = workspace::AttemptFailure::new(
                    workspace::CODE_RESULT_TOO_LARGE,
                    format!("{serialize_context}: {detail}"),
                    recovery,
                );
                return self.typed_failure(failure, failure_version);
            }
        };
        self.bounded_tool_envelope(
            envelope,
            failure_version,
            serialize_context,
            recovery,
            max_envelope_bytes,
        )
    }

    /// The shared response bound for an already-built tool envelope, used
    /// by tools whose envelope comes from the protocol layer instead of
    /// [`tool_result`](protocol::tool_result). The budget is the caller's
    /// id-aware envelope budget (#7768), so the final re-measurement after
    /// a wrapper added text (the status recovery sentence) still cannot
    /// pass an envelope the wire would have to replace.
    fn bounded_tool_envelope(
        &self,
        envelope: Value,
        failure_version: &'static str,
        serialize_context: &'static str,
        recovery: &'static str,
        max_envelope_bytes: usize,
    ) -> Result<CallToolResponse, ErrorData> {
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|_error| ErrorData::internal_error(serialize_context, None))?;
        if bytes.len() > max_envelope_bytes {
            let failure = workspace::AttemptFailure::new(
                workspace::CODE_RESULT_TOO_LARGE,
                format!(
                    "tool response is {} bytes; the id-aware MCP response envelope budget is {max_envelope_bytes} of the {}-byte bound",
                    bytes.len(),
                    super::MAX_RESPONSE_BYTES
                ),
                recovery,
            );
            return self.typed_failure(failure, failure_version);
        }
        let mut result: CallToolResult = typed(envelope)?;
        result.result_type = Some(ResultType::COMPLETE);
        Ok(result.into())
    }
}

fn optional_string_argument(
    arguments: &Option<serde_json::Map<String, Value>>,
    name: &str,
) -> Result<Option<String>, ErrorData> {
    let Some(arguments) = arguments else {
        return Ok(None);
    };
    match arguments.get(name) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_other) => Err(ErrorData::invalid_params(
            format!("{name} must be a string"),
            None,
        )),
    }
}

/// A filter string (#7464): a non-empty JSON string. Empty strings stay
/// invalid params instead of silently matching nothing or everything.
fn optional_nonempty_string_argument(
    arguments: &Option<serde_json::Map<String, Value>>,
    name: &str,
) -> Result<Option<String>, ErrorData> {
    match optional_string_argument(arguments, name)? {
        Some(value) if value.is_empty() => Err(ErrorData::invalid_params(
            format!("{name} must be a non-empty string"),
            None,
        )),
        other => Ok(other),
    }
}

/// A `repair_ready` selector (#7464): a JSON boolean. Strings, integers and
/// nulls stay invalid params instead of silently coercing.
fn optional_bool_argument(
    arguments: &Option<serde_json::Map<String, Value>>,
    name: &str,
) -> Result<Option<bool>, ErrorData> {
    let Some(arguments) = arguments else {
        return Ok(None);
    };
    match arguments.get(name) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_other) => Err(ErrorData::invalid_params(
            format!("{name} must be a boolean"),
            None,
        )),
    }
}

/// A paging offset (#6021): a JSON integer `>= 0`. Booleans, floats,
/// strings and negatives stay invalid params instead of silently coercing.
fn optional_nonnegative_argument(
    arguments: &Option<serde_json::Map<String, Value>>,
    name: &str,
) -> Result<Option<usize>, ErrorData> {
    let Some(arguments) = arguments else {
        return Ok(None);
    };
    match arguments.get(name) {
        None => Ok(None),
        Some(value) => {
            let raw = value.as_u64().ok_or_else(|| {
                ErrorData::invalid_params(format!("{name} must be a non-negative integer"), None)
            })?;
            usize::try_from(raw).map(Some).map_err(|error| {
                ErrorData::invalid_params(format!("{name} is out of range: {error}"), None)
            })
        }
    }
}

/// A paging limit (#6021): a JSON integer `>= 1`.
fn optional_positive_argument(
    arguments: &Option<serde_json::Map<String, Value>>,
    name: &str,
) -> Result<Option<usize>, ErrorData> {
    match optional_nonnegative_argument(arguments, name)? {
        Some(0) => Err(ErrorData::invalid_params(
            format!("{name} must be at least 1"),
            None,
        )),
        other => Ok(other),
    }
}

/// A paging limit with a typed row cap: a JSON integer `>= 1` and `<= max`.
/// A request above the cap stays invalid params at the dispatch edge instead
/// of being silently clamped, so the typed `page.limit` echo always names a
/// limit the server actually honors (#8735).
fn optional_capped_page_limit(
    arguments: &Option<serde_json::Map<String, Value>>,
    name: &str,
    max: usize,
) -> Result<Option<usize>, ErrorData> {
    match optional_positive_argument(arguments, name)? {
        Some(value) if value > max => Err(ErrorData::invalid_params(
            format!("{name} must be at most {max}"),
            None,
        )),
        other => Ok(other),
    }
}

/// Resolve one captured resource-id segment through an exact-id lookup with
/// the #8736 URI-decoding rules and the collision fail-closed follow-up:
///
/// - the raw capture is tried first (legacy raw URI support);
/// - when the raw capture misses with `item_not_found`, the segment decodes
///   once and the decoded form is tried instead;
/// - when the raw capture succeeds *and* the segment also decodes to a
///   different existing id, both spellings name two distinct items (for
///   example `a%20b.py` raw versus `a b.py` decoded), so the route fails
///   closed with `id_ambiguous` instead of silently picking one — the wire
///   form cannot say which file the client meant.
///
/// Recovery for every ambiguous or unresolved read names the equivalent
/// tool route, which takes the canonical id as a plain JSON string with no
/// URI encoding.
fn resource_id_document(
    captured: &str,
    lookup: impl Fn(&str) -> Result<Value, workspace::AttemptFailure>,
) -> Result<Value, workspace::AttemptFailure> {
    match lookup(captured) {
        Ok(document) => {
            if let Some(decoded) = gaps::percent_decode_resource_id(captured)
                && lookup(&decoded).is_ok()
            {
                return Err(workspace::AttemptFailure::new(
                    workspace::CODE_ID_AMBIGUOUS,
                    format!(
                        "the resource id is ambiguous: the captured segment resolves both as sent ({captured}) and as its percent-decoded form ({decoded}), which name two distinct canonical items"
                    ),
                    "read the item through the equivalent tool (ripr_get_gap or ripr_get_repair_card) with the canonical_id as a plain JSON string, which carries no URI encoding",
                ));
            }
            Ok(document)
        }
        Err(failure) if failure.code == workspace::CODE_ITEM_NOT_FOUND => {
            match gaps::percent_decode_resource_id(captured) {
                Some(decoded) => lookup(&decoded),
                None => Err(failure),
            }
        }
        Err(failure) => Err(failure),
    }
}

/// The declared input schemas set `additionalProperties: false`; the adapter
/// enforces the same bound at the dispatch edge instead of trusting the SDK
/// or the client to reject unknown argument names.
fn reject_unknown_arguments(
    arguments: &Option<serde_json::Map<String, Value>>,
    allowed: &[&str],
) -> Result<(), ErrorData> {
    let Some(arguments) = arguments else {
        return Ok(());
    };
    for name in arguments.keys() {
        if !allowed.contains(&name.as_str()) {
            return Err(ErrorData::invalid_params(
                format!("unknown argument {name:?}; allowed: {allowed:?}"),
                None,
            ));
        }
    }
    Ok(())
}

fn required_string_argument(
    arguments: &Option<serde_json::Map<String, Value>>,
    name: &str,
) -> Result<String, ErrorData> {
    let value = optional_string_argument(arguments, name)?;
    value.filter(|value| !value.is_empty()).ok_or_else(|| {
        ErrorData::invalid_params(
            format!("{name} is required and must be a non-empty string"),
            None,
        )
    })
}

fn typed<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, ErrorData> {
    serde_json::from_value(value)
        .map_err(|_error| ErrorData::internal_error("invalid status projection", None))
}

fn repair_document_from_session(
    session: &workspace::WorkspaceSession,
    id: &str,
    receipt: bool,
    root: Option<&std::path::Path>,
    root_identity: Option<&str>,
) -> Result<Value, workspace::AttemptFailure> {
    if receipt {
        session.receipt_status_document(id, root, root_identity)
    } else {
        session.repair_attempt_document(id, root, root_identity)
    }
}

async fn blocking_repair_read(
    read: impl FnOnce() -> Result<Value, workspace::AttemptFailure> + Send + 'static,
) -> Result<Value, workspace::AttemptFailure> {
    tokio::task::spawn_blocking(read).await.map_err(|error| {
        workspace::AttemptFailure::new(
            workspace::CODE_ANALYSIS_FAILED,
            format!("durable repair read worker failed: {error}"),
            "retry the exact attempt or receipt read",
        )
    })?
}

impl ServerHandler for McpServer {
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools
            .tools
            .iter()
            .find(|tool| tool.name == name)
            .cloned()
    }
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(protocol::INSTRUCTIONS)
        .with_server_info(Implementation::new("ripr", env!("CARGO_PKG_VERSION")))
    }
    /// The SDK decodes a request whose params fail typed validation as a
    /// custom request and the default answers `-32601` with only the method
    /// name. A method this server serves is not missing: report the shape
    /// violation as `-32602` so a client can tell a bad call from a bad
    /// method (#8504). `initialize` is served too: a repeated `initialize`
    /// without its required params is a params error, not a missing method.
    /// A method that is missing says so conventionally (`Method not found:`).
    async fn on_custom_request(
        &self,
        request: CustomRequest,
        _: RequestContext<RoleServer>,
    ) -> Result<CustomResult, ErrorData> {
        const SERVED_METHODS: [&str; 9] = [
            "initialize",
            "server/discover",
            "completion/complete",
            "tools/call",
            "tools/list",
            "resources/read",
            "resources/list",
            "resources/templates/list",
            "prompts/list",
        ];
        if SERVED_METHODS.contains(&request.method.as_str()) {
            let message = if request.method == "initialize" {
                "invalid params for `initialize`: expected `protocolVersion`, `capabilities` and `clientInfo`"
                    .to_owned()
            } else {
                format!("invalid params for `{}`", request.method)
            };
            return Err(ErrorData::invalid_params(message, None));
        }
        Err(ErrorData::new(
            ErrorCode::METHOD_NOT_FOUND,
            format!("Method not found: {}", request.method),
            None,
        ))
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(self.tools.clone())
    }
    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(self.resources.clone())
    }
    async fn list_resource_templates(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(self.resource_templates.clone())
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        // Every producer measures against this one id-aware budget (#7768):
        // the response bound minus the JSON-escaped request id of the call
        // being answered minus the fixed outer reserve.
        let max_envelope_bytes = super::outer_envelope_budget(&context.id);
        match request.name.as_ref() {
            protocol::STATUS_TOOL_NAME => {
                if request
                    .arguments
                    .as_ref()
                    .is_some_and(|arguments| !arguments.is_empty())
                {
                    return Err(ErrorData::invalid_params(
                        "ripr_workspace_status does not accept arguments",
                        None,
                    ));
                }
                self.status_tool(max_envelope_bytes).await
            }
            protocol::REFRESH_TOOL_NAME => {
                if request
                    .arguments
                    .as_ref()
                    .is_some_and(|arguments| !arguments.is_empty())
                {
                    return Err(ErrorData::invalid_params(
                        "ripr_refresh does not accept arguments",
                        None,
                    ));
                }
                self.refresh_tool(max_envelope_bytes).await
            }
            protocol::LIST_GAPS_TOOL_NAME => {
                self.list_gaps_tool(request.arguments, max_envelope_bytes)
                    .await
            }
            protocol::GET_GAP_TOOL_NAME => {
                self.get_gap_tool(request.arguments, max_envelope_bytes)
                    .await
            }
            protocol::PREPARE_REPAIR_TOOL_NAME => {
                self.prepare_repair_tool(request.arguments, max_envelope_bytes)
                    .await
            }
            protocol::GET_REPAIR_ATTEMPT_TOOL_NAME => {
                self.get_repair_attempt_tool(request.arguments, max_envelope_bytes)
                    .await
            }
            protocol::GET_RECEIPT_STATUS_TOOL_NAME => {
                self.get_receipt_status_tool(request.arguments, max_envelope_bytes)
                    .await
            }
            repair_card::GET_REPAIR_CARD_TOOL_NAME => {
                self.get_repair_card_tool(request.arguments, max_envelope_bytes)
                    .await
            }
            super::attempt_inventory::TOOL_NAME => {
                self.list_repair_attempts_tool(request.arguments, max_envelope_bytes)
                    .await
            }
            // MCP: an unknown tool is a protocol error with Invalid Params
            // (-32602), not Method Not Found; the method `tools/call` exists.
            _other => Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "unknown tool; available: ripr_workspace_status, ripr_refresh, ripr_list_gaps, ripr_get_gap, ripr_prepare_repair, ripr_get_repair_attempt, ripr_get_receipt_status, ripr_get_repair_card, ripr_list_repair_attempts",
                Some(serde_json::json!({
                    "available": [
                        protocol::STATUS_TOOL_NAME,
                        protocol::REFRESH_TOOL_NAME,
                        protocol::LIST_GAPS_TOOL_NAME,
                        protocol::GET_GAP_TOOL_NAME,
                        protocol::PREPARE_REPAIR_TOOL_NAME,
                        protocol::GET_REPAIR_ATTEMPT_TOOL_NAME,
                        protocol::GET_RECEIPT_STATUS_TOOL_NAME,
                        repair_card::GET_REPAIR_CARD_TOOL_NAME,
                        super::attempt_inventory::TOOL_NAME,
                    ]
                })),
            )),
        }
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        // The same id-aware budget the tool path uses (#7768): a resource
        // read answers one request whose echoed id rides the same wire cap.
        let max_envelope_bytes = super::outer_envelope_budget(&context.id);
        if request.uri == protocol::STATUS_RESOURCE_URI {
            let session = self.session.lock().await;
            let result = protocol::status_resource_result(
                &self.status,
                &session,
                &self.profile,
                super::MAX_MESSAGE_BYTES,
                super::MAX_RESPONSE_BYTES,
            )
            .map_err(|_error| ErrorData::internal_error("serialize workspace status", None))?;
            let bytes = serde_json::to_vec(&result)
                .map_err(|_error| ErrorData::internal_error("serialize resource", None))?
                .len();
            if bytes > max_envelope_bytes {
                return Err(resource_budget_failure(
                    bytes,
                    max_envelope_bytes,
                    workspace::RECOVERY_STATUS_TOO_LARGE,
                ));
            }
            let result: ReadResourceResult = typed(result)?;
            return Ok(ReadResourceResult::new(result.contents)
                .with_ttl_ms(0)
                .with_cache_scope(CacheScope::Private)
                .into());
        }
        if let Some(snapshot_id) = gaps::snapshot_resource_id(&request.uri) {
            let session = self.session.lock().await;
            return match session.snapshot_document(snapshot_id) {
                Ok(document) => self.resource_result(
                    document,
                    &request.uri,
                    workspace::RECOVERY_SNAPSHOT_TOO_LARGE,
                    max_envelope_bytes,
                ),
                Err(failure) => Err(resource_failure(
                    "snapshot",
                    &failure,
                    session
                        .last_good
                        .as_ref()
                        .map(|snapshot| snapshot.snapshot_id.as_str()),
                )),
            };
        }
        if let Some(canonical_id) = gaps::gap_resource_id(&request.uri) {
            let session = self.session.lock().await;
            let result = resource_id_document(canonical_id, |id| session.get_gap(id, None));
            return match result {
                Ok(document) => self.resource_result(
                    document,
                    &request.uri,
                    workspace::RECOVERY_GAP_ITEM_TOO_LARGE,
                    max_envelope_bytes,
                ),
                Err(failure) => Err(resource_failure("gap", &failure, None)),
            };
        }
        if let Some(attempt_id) = repair::repair_attempt_resource_id(&request.uri) {
            return match self.repair_read_document(attempt_id, false).await {
                Ok(document) => self.resource_result(
                    document,
                    &request.uri,
                    workspace::RECOVERY_REPAIR_TOO_LARGE,
                    max_envelope_bytes,
                ),
                Err(failure) => Err(resource_failure("repair-attempt", &failure, None)),
            };
        }
        if let Some(receipt_id) = repair::receipt_resource_id(&request.uri) {
            return match self.repair_read_document(receipt_id, true).await {
                Ok(document) => self.resource_result(
                    document,
                    &request.uri,
                    workspace::RECOVERY_REPAIR_TOO_LARGE,
                    max_envelope_bytes,
                ),
                Err(failure) => Err(resource_failure("receipt", &failure, None)),
            };
        }
        if let Some(item_id) = repair_card::repair_card_resource_id(&request.uri) {
            let session = self.session.lock().await;
            let root = self.analysis_root.clone();
            let result = resource_id_document(item_id, |id| {
                session.repair_card_document(id, None, root.as_deref())
            });
            return match result {
                Ok(document) => self.resource_result(
                    document,
                    &request.uri,
                    workspace::RECOVERY_REPAIR_TOO_LARGE,
                    max_envelope_bytes,
                ),
                Err(failure) => Err(resource_failure("repair-card", &failure, None)),
            };
        }
        let resource_templates = [
            protocol::SNAPSHOT_RESOURCE_TEMPLATE,
            protocol::GAP_RESOURCE_TEMPLATE,
            repair::REPAIR_ATTEMPT_TEMPLATE,
            repair::RECEIPT_TEMPLATE,
            repair_card::REPAIR_CARD_TEMPLATE,
        ];
        Err(ErrorData::resource_not_found(
            format!(
                "unknown resource; available: {}; templates: {}",
                protocol::STATUS_RESOURCE_URI,
                resource_templates.join(", ")
            ),
            Some(serde_json::json!({
                "available": [protocol::STATUS_RESOURCE_URI],
                "resource_templates": resource_templates,
            })),
        ))
    }
}

impl McpServer {
    fn resource_result(
        &self,
        document: Value,
        uri: &str,
        recovery: &'static str,
        max_envelope_bytes: usize,
    ) -> Result<ReadResourceResponse, ErrorData> {
        // Resources pretty-print their document text while tool payloads
        // serialize compact (`bounded_tool_envelope`): the encodings differ
        // by design, and tool/resource equivalence (#7145) is pinned over the
        // parsed document, never over the wire bytes. Do not reformat either
        // side to match the other.
        let text = serde_json::to_string_pretty(&document)
            .map_err(|_error| ErrorData::internal_error("serialize resource", None))?;
        let envelope = json_envelope(uri, text);
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|_error| ErrorData::internal_error("serialize resource", None))?
            .len();
        if bytes > max_envelope_bytes {
            return Err(resource_budget_failure(bytes, max_envelope_bytes, recovery));
        }
        let result: ReadResourceResult = typed(envelope)?;
        Ok(ReadResourceResult::new(result.contents)
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private)
            .into())
    }
}

/// The typed resource refusal for an envelope that cannot fit the caller's
/// id-aware response budget (#7768): the same `result_too_large` code and
/// per-surface recovery the tool path returns, rendered through the
/// standard resource failure shape.
fn resource_budget_failure(
    bytes: usize,
    max_envelope_bytes: usize,
    recovery: &'static str,
) -> ErrorData {
    let failure = workspace::AttemptFailure::new(
        workspace::CODE_RESULT_TOO_LARGE,
        format!(
            "resource response is {bytes} bytes; the id-aware MCP response envelope budget is {max_envelope_bytes} of the {}-byte bound",
            super::MAX_RESPONSE_BYTES
        ),
        recovery,
    );
    resource_failure("bounded", &failure, None)
}

fn json_envelope(uri: &str, text: String) -> Value {
    serde_json::json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": text
        }]
    })
}

/// Standard resource-miss semantics with the typed failure as data: the
/// message stays bounded and human-readable, and `data.code` carries the
/// typed state (`no_snapshot`, `stale_snapshot`, `item_not_found`, …).
fn resource_failure(
    kind: &str,
    failure: &workspace::AttemptFailure,
    current_snapshot_id: Option<&str>,
) -> ErrorData {
    let mut data = serde_json::json!({
        "code": failure.code,
        "recovery": failure.recovery,
        "detail": failure.detail,
        "failure": failure.value(),
    });
    if let Some(snapshot_id) = current_snapshot_id {
        data["current_snapshot_id"] = Value::from(snapshot_id);
    }
    ErrorData::resource_not_found(
        format!("unavailable {kind} resource: {}", failure.code),
        Some(data),
    )
}

/// Serve adapter that answers `ping` by method name on an `initialize`
/// session. The pinned SDK treats a post-init ping whose `params._meta`
/// names `2026-07-28` as a discovery-lifecycle request and replies
/// `-32601`; pre-init ping already bypasses that match. Discovery sessions
/// keep ping as method-not-found.
pub(super) struct InitializeSessionService {
    inner: McpServer,
}

impl InitializeSessionService {
    pub(super) fn new(
        status: WorkspaceStatus,
        analysis_root: Option<PathBuf>,
    ) -> Result<Self, ErrorData> {
        Ok(Self {
            inner: McpServer::new(status, analysis_root)?,
        })
    }
}

fn initialize_session_answers_ping(context: &RequestContext<RoleServer>) -> bool {
    context
        .peer
        .peer_info()
        .is_some_and(|info| info.protocol_version.has_initialize())
}

async fn answer_initialize_session_ping(
    handler: &McpServer,
    context: RequestContext<RoleServer>,
) -> Result<ServerResult, ErrorData> {
    if !initialize_session_answers_ping(&context) {
        return Err(ErrorData::method_not_found::<PingRequestMethod>());
    }
    let mut result = handler.ping(context).await.map(ServerResult::empty)?;
    // Initialize peers are older than `2026-07-28`; keep the empty `{}`
    // wire shape the existing stdio ping control asserts.
    result.strip_result_type_for_legacy_peer();
    Ok(result)
}

impl rmcp::Service<RoleServer> for InitializeSessionService {
    async fn handle_request(
        &self,
        request: ClientRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<ServerResult, ErrorData> {
        if matches!(request, ClientRequest::PingRequest(_)) {
            return answer_initialize_session_ping(&self.inner, context).await;
        }
        <McpServer as rmcp::Service<RoleServer>>::handle_request(&self.inner, request, context)
            .await
    }

    async fn handle_notification(
        &self,
        notification: ClientNotification,
        context: NotificationContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        <McpServer as rmcp::Service<RoleServer>>::handle_notification(
            &self.inner,
            notification,
            context,
        )
        .await
    }

    fn get_info(&self) -> ServerConfig {
        ServerHandler::get_info(&self.inner)
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        <McpServer as rmcp::Service<RoleServer>>::supported_protocol_versions(&self.inner)
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
