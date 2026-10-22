use crate::workspace_status::{WorkspaceState, WorkspaceStatus};
use serde::Serialize;
use serde_json::{Value, json};

use super::gaps::GAP_SCHEMA_VERSION;
use super::repair::{
    CODE_ATTEMPT_INVALID, CODE_ATTEMPT_NOT_FOUND, RECEIPT_STATUS_SCHEMA_VERSION, RECEIPT_TEMPLATE,
    REPAIR_ATTEMPT_SCHEMA_VERSION, REPAIR_ATTEMPT_TEMPLATE, REPAIR_PACKET_SCHEMA_VERSION,
};
use super::repair_card::{
    CODE_BUDGET_OVERFLOW, CODE_IDENTITY_UNNAMEABLE, CODE_INCOMPLETE_INVENTORY, CODE_POLICY_OMITTED,
    CODE_SEAM_NOT_FOUND, CODE_WITNESS_UNAVAILABLE, GET_REPAIR_CARD_TOOL_NAME,
    REPAIR_CARD_SCHEMA_VERSION, REPAIR_CARD_TEMPLATE,
};
use super::workspace::{
    AttemptFailure, CODE_NO_SNAPSHOT, REFRESH_SCHEMA_VERSION, RESERVED_FAILURE_CODES,
    SESSION_SCHEMA_VERSION, SessionProfile, WorkspaceSession,
};

pub(super) const STATUS_TOOL_NAME: &str = "ripr_workspace_status";
pub(super) const REFRESH_TOOL_NAME: &str = "ripr_refresh";
pub(super) const LIST_GAPS_TOOL_NAME: &str = "ripr_list_gaps";
pub(super) const GET_GAP_TOOL_NAME: &str = "ripr_get_gap";
pub(super) const PREPARE_REPAIR_TOOL_NAME: &str = "ripr_prepare_repair";
pub(super) const GET_REPAIR_ATTEMPT_TOOL_NAME: &str = "ripr_get_repair_attempt";
pub(super) const GET_RECEIPT_STATUS_TOOL_NAME: &str = "ripr_get_receipt_status";
pub(super) const STATUS_RESOURCE_URI: &str = "ripr://workspace/status";
pub(super) const SNAPSHOT_RESOURCE_TEMPLATE: &str = "ripr://snapshot/{snapshot_id}";
pub(super) const GAP_RESOURCE_TEMPLATE: &str = "ripr://gap/{canonical_id}";
/// The typed-failure vocabulary every evidence tool can return. Codes a
/// tool cannot reach in this slice stay named (reserved) so the wire
/// contract is stable when the owning slice lands; each tool description
/// says which codes it can actually return.
fn failure_vocabulary() -> Vec<&'static str> {
    let mut codes = vec![
        "workspace_unavailable",
        "analysis_failed",
        "unsupported_profile",
        super::workspace::CODE_CONFIG_INVALID,
        CODE_NO_SNAPSHOT,
        "analysis_in_flight",
        "stale_snapshot",
        "item_not_found",
        "result_too_large",
        CODE_ATTEMPT_NOT_FOUND,
        CODE_ATTEMPT_INVALID,
        super::attempt_inventory::CODE_STALE_INVENTORY,
    ];
    codes.extend(RESERVED_FAILURE_CODES.iter().copied());
    codes.extend([
        CODE_SEAM_NOT_FOUND,
        CODE_POLICY_OMITTED,
        CODE_WITNESS_UNAVAILABLE,
        CODE_IDENTITY_UNNAMEABLE,
        CODE_BUDGET_OVERFLOW,
        CODE_INCOMPLETE_INVENTORY,
    ]);
    codes
}

/// What a host shows a model before any call. Name what RIPR answers, what
/// this server does not, and the CLI route that runs analysis outside this
/// session. Naming a route is not invoking it: the server still edits
/// nothing and executes nothing (ADR 0022).
pub(super) const INSTRUCTIONS: &str = "RIPR is a static analyzer that asks whether the current tests would notice if the behavior changed in a diff were wrong. This MCP server exposes one read-only workspace session; it never edits source, executes nothing, runs no tests or mutation, and launches no processes; it names CLI routes without invoking them (ADR 0022). No document this server returns is a repair authorization: the external client's approval and sandbox policy remain authoritative. Flow: call `ripr_workspace_status` for root discovery, authority, and session facts; call `ripr_refresh` to analyze the committed default-branch diff and commit a completed snapshot. In this snapshot, staged and unstaged tracked edits and untracked files are outside the analyzed scope — `ripr check` defaults to the working tree when staged or unstaged tracked edits exist; shared check authority does not mean identical input; use `ripr check --committed` for committed input or `ripr check --format json` for working-tree input. Continue with `ripr_list_gaps` for the bounded working set; read one item's evidence with `ripr_get_gap` or ripr://gap/{canonical_id}. When repair-readiness facts are established, `ripr_prepare_repair` creates (or replays) one bounded in-memory repair transaction; read it with `ripr_get_repair_attempt` or ripr://repair-attempt/{attempt_id}, inspect receipt state with `ripr_get_receipt_status` or ripr://receipt/{receipt_id}, list every durable attempt with `ripr_list_repair_attempts`, and project the repair card with `ripr_get_repair_card` or ripr://repair-card/{canonical_id} — the same repair_card.v1 document `ripr agent card` and the language server project.";

const STATUS_TOOL_DESCRIPTION: &str = "Report the RIPR workspace and session state: repository-root discovery state (validated or unavailable, with root error detail when no usable Git work tree is detected), launch-trust and authority facts (source edit, verification execution, mutation execution, and model provider are all none), and a session block — current desired input, attempt state (no_snapshot, in_flight, completed, failed), last completed snapshot identity, last-known-good state, freshness, the typed AnalysisOutcome, and profile facts (project_config loaded with enabled languages when the workspace ripr.toml resolves, built-in defaults otherwise, an unreadable config detected_not_loaded). `workspace_state: ready` means only that a repository root was discovered — not that analysis ran or that no issues were found. This tool returns session facts only: no gap evidence. It never edits source, runs tests or mutation, executes verification commands, or loads project-local provider configuration. To run analysis, call ripr_refresh.";

const REFRESH_TOOL_DESCRIPTION: &str = "Run one bounded static analysis of the committed default-branch diff (optional `base` selects another revision) through RIPR's shared check authority and commit the completed snapshot into this server's session. staged and unstaged tracked edits and untracked files are outside the analyzed scope — `ripr check` defaults to the working tree when staged or unstaged tracked edits exist; shared check authority does not mean identical input; `ripr check --committed` covers committed input and `ripr check --worktree --format json` covers staged and unstaged edits outside MCP. The call blocks until a terminal attempt state: completed (snapshot identity returned), failed (typed code, bounded detail, recovery; last-known-good kept), in_flight (another attempt is running; wait and retry), or workspace_unavailable (restart the server with `--root <repository>`). An unresolvable default branch fails with analysis_failed naming candidate branches; retry with `base`. `base` starting with `-` or naming no commit fails closed; omitting `base` returns to the default branch. A cancelled attempt still commits as a completed snapshot when it finishes and only transport teardown abandons one before it commits; a superseded attempt is never committed. An unsupported client profile fails closed with unsupported_profile. This tool never edits source, executes verification or mutation commands, or prepares a repair.";

const LIST_GAPS_TOOL_DESCRIPTION: &str = "Return the deterministic bounded working set of canonical items for the current completed snapshot (optional snapshot_id must match or the call fails closed with stale_snapshot). The response carries counts, omitted identities with reasons, the selection basis, and one small summary per selected item; the outputSchema enumerates the fields. Selection is the shared CLI/LSP budget authority: MCP does not re-rank, never truncates silently, and infers no business risk; overflow is disclosed with reasons and the omitted identities. Optional file, class, language, and repair_ready filters narrow the stored selection before paging (conjunctive); offset and limit page the narrowed set in snapshot order, disclosing the window through page.has_more and page.next_offset. Before the first successful ripr_refresh this fails closed with no_snapshot; unavailable roots fail closed with workspace_unavailable (restart with `ripr mcp --stdio --root <repository>`); an over-bound page fails with result_too_large. Per-item detail: ripr_get_gap or ripr://gap/{canonical_id}.";

const GET_GAP_TOOL_DESCRIPTION: &str = "Return one canonical item's complete bounded analysis evidence from the current completed snapshot (optional snapshot_id must match or the call fails closed with stale_snapshot). This is the read for analysis evidence when no repair intent exists — prefer ripr_get_repair_card when the intent is to repair; this evidence never authorizes an edit. repair_packet_ready now reports the committed producer repair-readiness facts. Unknown ids fail closed with item_not_found; a document over the response bound fails with result_too_large — recovery is `ripr check --committed --format json` outside MCP (a bounded summary whose related tests cap per finding); retrying ripr_get_gap cannot shrink the item. Before the first successful ripr_refresh this fails closed with no_snapshot; unavailable roots fail closed with workspace_unavailable (restart with `ripr mcp --stdio --root <repository>`). The resource ripr://gap/{canonical_id} returns the same document.";

const PREPARE_REPAIR_TOOL_DESCRIPTION: &str = "Evaluate the committed producer repair-readiness facts for one canonical item of the current completed snapshot and, only when every gate is established, create — or replay — one bounded in-memory repair transaction bound to that snapshot and item. Inputs: canonical_id (required, from ripr_list_gaps) and optional snapshot_id, which must match the current snapshot or the call fails closed with stale_snapshot. The outputSchema enumerates the complete document (attempt id, identities, fix site, edit surface, must_not_change, stop_conditions, before-evidence identity, empty typed command_routes, limitations, resource links). Repeating the call for the same snapshot, item, and root returns the identical document and never creates a second transaction. When a gate is not established the tool returns repair_packet_ready: false with the typed ineligibility reason and attempt: null — it never guesses a missing field and never creates a misleading attempt. This tool never edits source, never launches a process, and never executes verification or mutation commands; the external client's approval and sandbox policy remains authoritative. Before the first successful ripr_refresh this fails closed with no_snapshot; unavailable roots fail closed with workspace_unavailable (restart with `ripr mcp --stdio --root <repository>`); unknown canonical ids with item_not_found.";

const GET_REPAIR_ATTEMPT_TOOL_DESCRIPTION: &str = "Read one repair transaction by its attempt identity, without executing anything it names. Session transactions from ripr_prepare_repair answer first: immutable packet, state awaiting_edit, snapshot and root identity binding, resource links. Otherwise the durable attempt store of this workspace root is inventoried through the shared repair-attempt authority: a valid manifest projects its state, repository head, seam identity, artifact digest bindings, follow-up command display string, and typed CommandSpec routes when the retained packet carries valid ones — the human display is never execution authority and shell_required or manual modes are visibly non-direct; the host-local root path is never projected. A manifest that fails canonical validation fails closed with attempt_invalid; an unknown identity fails closed with attempt_not_found. An attempt prepared against a snapshot that is no longer current fails closed with the reserved superseded state and the current snapshot identity. Superseded tombstones older than the 64-entry session bound read attempt_not_found instead. This tool never edits source, never launches a process, and never executes verification or mutation commands. The resource ripr://repair-attempt/{attempt_id} returns the same document.";

const GET_RECEIPT_STATUS_TOOL_DESCRIPTION: &str = "Read the current receipt state for one attempt identity (receipt ids are attempt-bound: one retained receipt per durable attempt). The status vocabulary is awaiting_edit, after_pending, verification_pending, improved, closed, unchanged, regressed, limited, stale, and invalid. Session transactions report awaiting_edit with an explicit null receipt: RIPR performs no verification and issues no receipt. Durable attempts report their producer state mapped onto the same vocabulary, and a finished digest-bound terminal receipt projects the receipt document with exact byte bindings and a terminal status owned by the same reading as CLI status: advisory plus improved static grip reports improved, complete changed stays open as limited, incomplete producer completeness stays limited except recorded stale or gap-mismatch lifecycle states retain stale or invalid, and invalid producer status stays invalid; advisory regression preserves the trimmed, case-insensitive legacy static_movement.state fallback; receipt presence alone never reports closed; a manifest or receipt that fails canonical validation reports invalid or attempt_invalid rather than a reconstructed state. Status binds exact before/after/verify bytes, repository identity, candidate item, and currentness, re-validated on every read — never joined by mtime or latest-file convention. Static movement and focused runtime test execution remain separate evidence axes — this document reports static receipt state only. This read edits nothing and executes nothing. The resource ripr://receipt/{receipt_id} returns the same document.";

const STATUS_RESOURCE_DESCRIPTION: &str = "Bounded, read-only workspace discovery, authority, and session status: repository-root discovery state, configuration presence, launch-trust and authority facts, and the current analysis session (attempt state, last completed snapshot identity, typed AnalysisOutcome, profile facts). No gap evidence detail is exposed here; run ripr_refresh, then read items through ripr_get_gap. On an unavailable root the payload carries the same recovery sentence as the ripr_workspace_status tool result as a second text content entry, naming the cause and the restart route (`ripr mcp --stdio --root <repository>`).";

const SNAPSHOT_RESOURCE_TEMPLATE_DESCRIPTION: &str = "Bounded evidence for one completed snapshot: snapshot identity, typed AnalysisOutcome, the canonical item index (identities and locations, not evidence), and the stored selection summary. One item's complete evidence: ripr_get_gap or ripr://gap/{canonical_id}. Unknown or superseded ids fail closed with no_snapshot or stale_snapshot; an unavailable root fails closed with workspace_unavailable — restart the server with `ripr mcp --stdio --root <repository>`.";

const GAP_RESOURCE_TEMPLATE_DESCRIPTION: &str = "One canonical item's complete evidence — changed behavior, causal attribution, discriminator availability, related tests, and the committed producer repair-readiness block; the same document ripr_get_gap returns. The item must exist in the current completed snapshot (else item_not_found). A document over the response bound fails with result_too_large — recovery is `ripr check --committed --format json` outside MCP; retrying this resource cannot shrink the item.";

const REPAIR_ATTEMPT_TEMPLATE_DESCRIPTION: &str = "One repair transaction, as returned by ripr_get_repair_attempt: a session transaction from ripr_prepare_repair or a durable manifest with digest bindings and typed CommandSpec routes when valid. Unknown identities fail closed (attempt_not_found), invalid manifests (attempt_invalid), superseded-snapshot bindings (superseded), and tombstones past the 64-entry bound (attempt_not_found). The read never edits source or executes anything the attempt names.";

const RECEIPT_TEMPLATE_DESCRIPTION: &str = "The receipt status document for one attempt identity, identical to the ripr_get_receipt_status tool result: the status vocabulary (awaiting_edit, after_pending, verification_pending, improved, closed, unchanged, regressed, limited, stale, invalid), the digest-bound receipt document when a durable attempt retained one, and the currentness basis. Receipt issuance is external authority — RIPR performs no verification and executes nothing on this read.";

const REPAIR_CARD_TEMPLATE_DESCRIPTION: &str = "One canonical item's bounded repair card, the same document ripr_get_repair_card returns: the repair_card.v1 document `ripr agent card` and the language server project assemble from the committed snapshot by the shared authority. The card names the seam, witness, fix instruction, edit cage, attempt state, and typed next action. The item must exist in the current snapshot with an owner-discriminated seam binding (else item_not_found or seam_not_found).";

const GET_REPAIR_CARD_TOOL_DESCRIPTION: &str = "Project the bounded repair card for one canonical item of the current completed snapshot: the same versioned repair_card.v1 document `ripr agent card` and the standard language server project, assembled by the shared application authority from the committed snapshot. Route disambiguation: prefer this card over ripr_get_gap when the agent intends to repair; ripr_get_gap is the lighter read when only analysis evidence is wanted. Inputs: canonical_id (required, from ripr_list_gaps) and optional snapshot_id, which must match the current snapshot or the call fails closed with stale_snapshot. The document carries the verbatim card (seam subject, changed behavior, exact blocker or named limitation, fix instruction, edit-cage surfaces, done-when goals, attempt state, typed next action) plus the claim boundary and resource links. The durable attempt store is re-read at card-read time, so the attempt block matches `ripr agent status` at this moment; in-memory session transactions never ride a card. The card edits nothing, executes nothing, and is never a repair authorization: the external client's approval and sandbox policy remains authoritative for the command the next action names, and the display binds the portable root `.` (never the host-local path). Reachable failure vocabulary: no_snapshot, analysis_in_flight, stale_snapshot, item_not_found, seam_not_found, identity_unnameable, budget_overflow, result_too_large, workspace_unavailable, analysis_failed; policy_omitted and witness_unavailable stay named on the wire for stability. The resource ripr://repair-card/{canonical_id} returns the same document.";

pub(super) fn tools_list_result() -> Value {
    json!({"tools": [
        status_tool_descriptor(),
        refresh_tool_descriptor(),
        list_gaps_tool_descriptor(),
        get_gap_tool_descriptor(),
        prepare_repair_tool_descriptor(),
        get_repair_attempt_tool_descriptor(),
        get_receipt_status_tool_descriptor(),
        get_repair_card_tool_descriptor(),
        super::attempt_inventory::tool_descriptor(),
    ]})
}

pub(super) fn resources_list_result() -> Value {
    json!({"resources": [status_resource_descriptor()]})
}

pub(super) fn resource_templates_list_result() -> Value {
    json!({"resourceTemplates": [
        {
            "uriTemplate": SNAPSHOT_RESOURCE_TEMPLATE,
            "name": "ripr-snapshot-evidence",
            "title": "RIPR snapshot evidence",
            "description": SNAPSHOT_RESOURCE_TEMPLATE_DESCRIPTION,
            "mimeType": "application/json",
        },
        {
            "uriTemplate": GAP_RESOURCE_TEMPLATE,
            "name": "ripr-gap-evidence",
            "title": "RIPR gap evidence",
            "description": GAP_RESOURCE_TEMPLATE_DESCRIPTION,
            "mimeType": "application/json",
        },
        {
            "uriTemplate": REPAIR_ATTEMPT_TEMPLATE,
            "name": "ripr-repair-attempt",
            "title": "RIPR repair attempt",
            "description": REPAIR_ATTEMPT_TEMPLATE_DESCRIPTION,
            "mimeType": "application/json",
        },
        {
            "uriTemplate": RECEIPT_TEMPLATE,
            "name": "ripr-receipt-status",
            "title": "RIPR receipt status",
            "description": RECEIPT_TEMPLATE_DESCRIPTION,
            "mimeType": "application/json",
        },
        {
            "uriTemplate": REPAIR_CARD_TEMPLATE,
            "name": "ripr-repair-card",
            "title": "RIPR repair card",
            "description": REPAIR_CARD_TEMPLATE_DESCRIPTION,
            "mimeType": "application/json",
        },
    ]})
}

#[derive(Serialize)]
struct McpStatusDocument<'a> {
    schema_version: &'static str,
    workspace: &'a WorkspaceStatus,
    session: Value,
    mcp: McpSurfaceStatus,
}

#[derive(Serialize)]
struct McpSurfaceStatus {
    transport: &'static str,
    tools: [&'static str; 9],
    resources: [&'static str; 1],
    resource_templates: [&'static str; 5],
    bounds: McpBoundsStatus,
}

#[derive(Serialize)]
struct McpBoundsStatus {
    max_message_bytes: usize,
    max_response_bytes: usize,
}

/// Build the status document for the tool result and the status resource.
pub(super) fn status_document(
    status: &WorkspaceStatus,
    session: &WorkspaceSession,
    profile: &SessionProfile,
    max_message_bytes: usize,
    max_response_bytes: usize,
) -> Value {
    serde_json::to_value(McpStatusDocument {
        schema_version: "ripr-mcp-workspace-status-v1",
        workspace: status,
        session: session.session_document(profile),
        mcp: McpSurfaceStatus {
            transport: "stdio",
            tools: [
                STATUS_TOOL_NAME,
                REFRESH_TOOL_NAME,
                LIST_GAPS_TOOL_NAME,
                GET_GAP_TOOL_NAME,
                PREPARE_REPAIR_TOOL_NAME,
                GET_REPAIR_ATTEMPT_TOOL_NAME,
                GET_RECEIPT_STATUS_TOOL_NAME,
                GET_REPAIR_CARD_TOOL_NAME,
                super::attempt_inventory::TOOL_NAME,
            ],
            resources: [STATUS_RESOURCE_URI],
            resource_templates: [
                SNAPSHOT_RESOURCE_TEMPLATE,
                GAP_RESOURCE_TEMPLATE,
                REPAIR_ATTEMPT_TEMPLATE,
                RECEIPT_TEMPLATE,
                REPAIR_CARD_TEMPLATE,
            ],
            bounds: McpBoundsStatus {
                max_message_bytes,
                max_response_bytes,
            },
        },
    })
    .unwrap_or_else(|_| {
        // Every field is producer-serializable by contract; a serialization
        // failure here is an instrument problem, and the status surface must
        // never panic (ADR 0022 bounded adapter). A minimal honest document
        // keeps the wire alive without fabricating facts.
        json!({
            "schema_version": "ripr-mcp-workspace-status-v1",
            "workspace": Value::Null,
            "session": Value::Null,
            "mcp": Value::Null,
        })
    })
}

/// The `ripr_workspace_status` tool envelope: the status document plus, when
/// the root is unavailable, one recovery sentence a host can show a model.
/// The document itself carries only `root.error_code`; the text content names
/// the cause and the restart route (the pinned single-tool behavior).
pub(super) fn status_tool_result(
    status: &WorkspaceStatus,
    session: &WorkspaceSession,
    profile: &SessionProfile,
    max_message_bytes: usize,
    max_response_bytes: usize,
    max_envelope_bytes: usize,
) -> Result<Value, String> {
    let document = status_document(
        status,
        session,
        profile,
        max_message_bytes,
        max_response_bytes,
    );
    let mut envelope = tool_result(document, max_envelope_bytes)?;
    if let Some(recovery) = unavailable_recovery(status)
        && let Some(content) = envelope.get_mut("content").and_then(Value::as_array_mut)
    {
        content.push(json!({
            "type": "text",
            "text": recovery
        }));
    }
    Ok(envelope)
}

/// A successful tool result envelope. The document is carried once at full
/// fidelity (#6021): `content[0].text` is the compact JSON serialization —
/// hosts parse it as JSON — and `structuredContent` repeats the same
/// document. Every tool here advertises an `outputSchema`, so a success
/// without its structured copy is an undeclared contract break (#7768):
/// when the complete double-carry envelope does not measure under
/// `max_envelope_bytes`, the caller's id-aware response budget
/// ([`super::outer_envelope_budget`]), this fails closed and the caller
/// answers with the typed `result_too_large` failure carrying its recovery
/// route. The former text-only fallback — ship the compact text, silently
/// drop `structuredContent` past the budget — made that loss
/// undetectable on the wire exactly where a document fit only as compact
/// text, so it was removed rather than marked (#7768). Paging callers
/// pre-shrink their pages against the same budget
/// ([`structured_envelope_overflows`]); single-item producers refuse
/// through their document guard and this envelope guard.
pub(super) fn tool_result(document: Value, max_envelope_bytes: usize) -> Result<Value, String> {
    let text = serde_json::to_string(&document)
        .map_err(|error| format!("render tool document: {error}"))?;
    let structured = json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": document,
        "isError": false,
    });
    let bytes = envelope_bytes(&structured)?;
    if bytes > max_envelope_bytes {
        return Err(format!(
            "tool document's complete envelope renders {bytes} bytes against the {max_envelope_bytes}-byte envelope budget ({}-byte response bound minus the request id and outer reserve); a success without structuredContent would break the advertised output schema",
            super::MAX_RESPONSE_BYTES
        ));
    }
    Ok(structured)
}

fn envelope_bytes(envelope: &Value) -> Result<usize, String> {
    struct LengthWriter(usize);

    impl std::io::Write for LengthWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0 += buffer.len();
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = LengthWriter(0);
    serde_json::to_writer(&mut writer, envelope)
        .map_err(|error| format!("measure tool envelope: {error}"))?;
    Ok(writer.0)
}

/// Whether the complete double-carry envelope for `document` measures over
/// `max_envelope_bytes`, the caller's id-aware response budget
/// ([`super::outer_envelope_budget`]) — the point where `tool_result`
/// fails closed instead of shipping a success. Paging callers shrink their
/// page until this is `false`, so a tool that advertises an `outputSchema`
/// always returns a structured result on the wire whatever request id the
/// client sent (#6021 review, #7768).
pub(super) fn structured_envelope_overflows(
    document: &Value,
    max_envelope_bytes: usize,
) -> Result<bool, String> {
    structured_envelope_bytes(document).map(|bytes| bytes > max_envelope_bytes)
}

/// Measure the shared full tool envelope. Callers compare these bytes
/// against the id-aware envelope budget, which reserves the actual request
/// ID and the fixed SDK/JSON-RPC wrapper (#7768).
pub(super) fn structured_envelope_bytes(document: &Value) -> Result<usize, String> {
    let text = serde_json::to_string(document)
        .map_err(|error| format!("render tool document: {error}"))?;
    let structured = json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": document,
        "isError": false,
    });
    envelope_bytes(&structured)
}

/// A typed tool failure envelope: standard `isError` semantics with the
/// failure as structured content and one recovery sentence in text form.
pub(super) fn tool_failure(
    failure: &AttemptFailure,
    schema_version: &str,
) -> Result<Value, String> {
    let document = failure.document(schema_version);
    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("{}: {} Recovery: {}", failure.code, failure.detail, failure.recovery),
        }],
        "structuredContent": document,
        "isError": true,
    }))
}

/// One sentence a model can act on when the root is unusable. The status
/// document carries only `root.error_code`; a host shows the text content, so
/// name the cause and the recovery there. The cause is the root owner's own
/// wording (`RootErrorCode::cause`). The server resolves its root once at
/// startup (ADR 0022), so every recovery is a restart with a different root.
pub(super) fn unavailable_recovery(status: &WorkspaceStatus) -> Option<String> {
    if status.workspace_state != WorkspaceState::Unavailable {
        return None;
    }
    let (code, cause) = match status.root.error_code {
        Some(code) => (code.as_str(), code.cause(status.root.source)),
        None => ("unknown", "the root could not be validated".to_string()),
    };
    Some(format!(
        "Workspace unavailable ({code}): {cause}. This server resolves its root once at startup; restart it with `ripr mcp --stdio --root <repository>` pointing at the repository to analyze."
    ))
}

/// The `ripr://workspace/status` resource payload. On an unavailable root
/// the same recovery sentence the tool result appends is carried as a
/// second text content entry (#8737): the JSON document itself stays the
/// bare status projection, but the restart route is no longer tool-only.
/// The entry rides inside `contents` because the read path round-trips the
/// payload through the SDK's typed resource result, which drops unknown
/// top-level fields.
pub(super) fn status_resource_result(
    status: &WorkspaceStatus,
    session: &WorkspaceSession,
    profile: &SessionProfile,
    max_message_bytes: usize,
    max_response_bytes: usize,
) -> Result<Value, String> {
    let document = status_document(
        status,
        session,
        profile,
        max_message_bytes,
        max_response_bytes,
    );
    let text = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("render workspace status: {error}"))?;
    let mut contents = vec![json!({
        "uri": STATUS_RESOURCE_URI,
        "mimeType": "application/json",
        "text": text
    })];
    if let Some(recovery) = unavailable_recovery(status) {
        contents.push(json!({
            "uri": STATUS_RESOURCE_URI,
            "mimeType": "text/plain",
            "text": recovery
        }));
    }
    Ok(json!({ "contents": contents }))
}

fn status_tool_descriptor() -> Value {
    json!({
        "name": STATUS_TOOL_NAME,
        "title": "RIPR workspace status",
        "description": STATUS_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        },
        "outputSchema": status_output_schema(),
        "annotations": {
            "title": "RIPR workspace status",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn refresh_tool_descriptor() -> Value {
    json!({
        "name": REFRESH_TOOL_NAME,
        "title": "RIPR refresh",
        "description": REFRESH_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "base": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 256,
                    "description": "Optional Git revision (branch, tag or commit) to diff against instead of the resolved default branch. Use it when the repository's default branch is not main or master, or when refresh fails with base_unresolvable. Omit it to use the default branch."
                }
            },
            "additionalProperties": false
        },
        "outputSchema": refresh_output_schema(),
        "annotations": {
            "title": "RIPR refresh",
            // #5192: readOnlyHint is scoped to the user's world (source,
            // tests, processes, external state). Committing the completed
            // snapshot mutates only in-memory session state, so the
            // annotation stands; idempotentHint:false discloses that
            // repeat calls advance the session.
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": false,
            "openWorldHint": true
        }
    })
}

fn list_gaps_tool_descriptor() -> Value {
    json!({
        "name": LIST_GAPS_TOOL_NAME,
        "title": "RIPR bounded gap list",
        "description": LIST_GAPS_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "snapshot_id": { "type": "string" },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Start index within the narrowed items, in snapshot order. Default 0."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum narrowed-item summaries to return in this page."
                },
                "file": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Narrow to items whose workspace-relative file equals, prefixes, or trailing-matches this path (one leading `./` is ignored on both sides)."
                },
                "class": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Narrow to one exposure-class wire token (for example `exposed` or `weakly_exposed`)."
                },
                "language": {
                    "type": "string",
                    "minLength": 1,
                    "enum": crate::domain::LanguageId::ALL
                        .iter()
                        .map(crate::domain::LanguageId::as_str)
                        .collect::<Vec<_>>(),
                    "description": "Narrow to one language wire string (for example `rust` or `python`)."
                },
                "repair_ready": {
                    "type": "boolean",
                    "description": "Narrow to repair-ready items (true) or not-ready items (false), using the commit-time readiness projection."
                }
            },
            "additionalProperties": false
        },
        "outputSchema": gap_list_output_schema(),
        "annotations": {
            "title": "RIPR bounded gap list",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn get_gap_tool_descriptor() -> Value {
    json!({
        "name": GET_GAP_TOOL_NAME,
        "title": "RIPR gap evidence",
        "description": GET_GAP_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "canonical_id": { "type": "string", "minLength": 1 },
                "snapshot_id": { "type": "string" }
            },
            "required": ["canonical_id"],
            "additionalProperties": false
        },
        "outputSchema": gap_output_schema(),
        "annotations": {
            "title": "RIPR gap evidence",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn prepare_repair_tool_descriptor() -> Value {
    json!({
        "name": PREPARE_REPAIR_TOOL_NAME,
        "title": "RIPR prepare repair",
        "description": PREPARE_REPAIR_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "canonical_id": { "type": "string", "minLength": 1 },
                "snapshot_id": { "type": "string" }
            },
            "required": ["canonical_id"],
            "additionalProperties": false
        },
        "outputSchema": repair_packet_output_schema(),
        "annotations": {
            "title": "RIPR prepare repair",
            // #5192: readOnlyHint is scoped to the user's world (source,
            // tests, processes, external state). The transaction is
            // in-memory, deterministic, and replayed identically, so the
            // annotation stands alongside idempotentHint:true.
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn get_repair_attempt_tool_descriptor() -> Value {
    json!({
        "name": GET_REPAIR_ATTEMPT_TOOL_NAME,
        "title": "RIPR repair attempt",
        "description": GET_REPAIR_ATTEMPT_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "attempt_id": { "type": "string", "minLength": 1 }
            },
            "required": ["attempt_id"],
            "additionalProperties": false
        },
        "outputSchema": repair_attempt_output_schema(),
        "annotations": {
            "title": "RIPR repair attempt",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn get_receipt_status_tool_descriptor() -> Value {
    json!({
        "name": GET_RECEIPT_STATUS_TOOL_NAME,
        "title": "RIPR receipt status",
        "description": GET_RECEIPT_STATUS_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "receipt_id": { "type": "string", "minLength": 1 }
            },
            "required": ["receipt_id"],
            "additionalProperties": false
        },
        "outputSchema": receipt_status_output_schema(),
        "annotations": {
            "title": "RIPR receipt status",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn get_repair_card_tool_descriptor() -> Value {
    json!({
        "name": GET_REPAIR_CARD_TOOL_NAME,
        "title": "RIPR repair card",
        "description": GET_REPAIR_CARD_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "canonical_id": { "type": "string", "minLength": 1 },
                "snapshot_id": { "type": "string" }
            },
            "required": ["canonical_id"],
            "additionalProperties": false
        },
        "outputSchema": repair_card_output_schema(),
        "annotations": {
            "title": "RIPR repair card",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn repair_card_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "schema_version": { "type": "string", "const": REPAIR_CARD_SCHEMA_VERSION },
                    "snapshot_id": { "type": "string" },
                    "requested_snapshot_id": { "type": ["string", "null"] },
                    "item": { "type": "object" },
                    "card": { "type": "object" },
                    "currentness": { "type": "object" },
                    "claim_boundary": { "type": "string" },
                    "limitations": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "links": { "type": "object" }
                },
                "required": [
                    "schema_version",
                    "snapshot_id",
                    "requested_snapshot_id",
                    "item",
                    "card",
                    "currentness",
                    "claim_boundary",
                    "limitations",
                    "links"
                ],
                "additionalProperties": false
            },
            typed_failure_document_schema(REPAIR_CARD_SCHEMA_VERSION)
        ]
    })
}

fn status_resource_descriptor() -> Value {
    json!({
        "uri": STATUS_RESOURCE_URI,
        "name": "ripr-workspace-status",
        "title": "RIPR workspace status",
        "description": STATUS_RESOURCE_DESCRIPTION,
        "mimeType": "application/json"
    })
}

fn failure_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "code": { "type": "string", "enum": failure_vocabulary() },
            "detail": { "type": "string" },
            "recovery": { "type": "string" },
            "data": { "type": "object" }
        },
        "required": ["code", "detail", "recovery", "data"],
        "additionalProperties": false
    })
}

pub(super) fn typed_failure_document_schema(schema_version: &str) -> Value {
    json!({
        "type": "object",
        "properties": {
            "schema_version": { "type": "string", "const": schema_version },
            "failure": failure_schema()
        },
        "required": ["schema_version", "failure"],
        "additionalProperties": false
    })
}

fn status_output_schema() -> Value {
    // Like every other tool schema: the success document, or the typed
    // failure the bound path returns for an oversized status (#6291).
    json!({
        "oneOf": [
            status_success_schema(),
            typed_failure_document_schema(SESSION_SCHEMA_VERSION)
        ]
    })
}

/// Scope facts emitted by `workspace::scope_disclosure` for a committed snapshot.
/// No snapshot or a clean tree emits null; untracked details are optional.
fn scope_disclosure_schema() -> Value {
    json!({
        "type": ["object", "null"],
        "properties": {
            "analyzed": {
                "type": "string",
                "pattern": "^committed (default-branch diff|diff against base .+)$"
            },
            "uncommitted_edits": { "type": "string", "const": "outside this analysis" },
            "note": { "type": "string" },
            "untracked_edits": {
                "type": "object",
                "properties": {
                    "files": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 3 },
                    "total": { "type": "integer", "minimum": 1 },
                    "more": { "type": "integer", "minimum": 0 },
                    "listing_example": { "type": "string" },
                    "remedy": { "type": "string" }
                },
                "required": ["files", "total", "more", "listing_example", "remedy"],
                "additionalProperties": false
            },
            "ignored_edits": {
                "type": "object",
                "properties": {
                    "files": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 3 },
                    "total": { "type": "integer", "minimum": 1 },
                    "more": { "type": "integer", "minimum": 0 },
                    "languages_analyzed": { "type": "string" },
                    "remedy": { "type": "string" }
                },
                "required": ["files", "total", "more", "languages_analyzed", "remedy"],
                "additionalProperties": false
            }
        },
        "required": ["analyzed", "uncommitted_edits", "note"],
        "additionalProperties": false
    })
}

fn status_success_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "schema_version": {
                "type": "string",
                "const": "ripr-mcp-workspace-status-v1"
            },
            "workspace": {
                "type": "object",
                "properties": {
                    "schema_version": {
                        "type": "string",
                        "const": "ripr-workspace-status-v1"
                    },
                    "workspace_state": {
                        "type": "string",
                        "enum": ["ready", "unavailable"]
                    },
                    "root": {
                        "type": "object",
                        "properties": {
                            "state": {
                                "type": "string",
                                "enum": ["validated", "unavailable"]
                            },
                            "source": {
                                "type": "string",
                                "enum": [
                                    "explicit",
                                    "current_directory",
                                    "ancestor_discovery",
                                    "unavailable"
                                ]
                            },
                            "identity": { "type": "string" },
                            "repository_markers": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "git_work_tree": {
                                "type": "string",
                                "enum": ["not_detected", "marker_invalid"]
                            },
                            "error_code": {
                                "type": "string",
                                "enum": [
                                    "current_directory_unavailable",
                                    "root_missing",
                                    "root_not_directory",
                                    "root_canonicalize_failed",
                                    "repository_marker_missing"
                                ]
                            }
                        },
                        "required": ["state", "source", "repository_markers"],
                        "additionalProperties": false
                    },
                    "configuration": {
                        "type": "object",
                        "properties": {
                            "project_config_state": {
                                "type": "string",
                                "enum": [
                                    "built_in_defaults_only",
                                    "detected_not_loaded",
                                    "loaded",
                                    "unavailable"
                                ]
                            },
                            "project_config_identity": {
                                "type": "string"
                            }
                        },
                        "required": ["project_config_state"],
                        "additionalProperties": false
                    },
                    "trust": {
                        "type": "object",
                        "properties": {
                            "project_config_trust": {
                                "type": "string",
                                "const": "not_established"
                            },
                            "effective_access": {
                                "type": "string",
                                "const": "read_only_status"
                            }
                        },
                        "required": ["project_config_trust", "effective_access"],
                        "additionalProperties": false
                    },
                    "authority": {
                        "type": "object",
                        "properties": {
                            "source_edit_capability": {
                                "type": "string",
                                "const": "none"
                            },
                            "verification_execution_capability": {
                                "type": "string",
                                "const": "none"
                            },
                            "mutation_execution_capability": {
                                "type": "string",
                                "const": "none"
                            },
                            "model_provider": {
                                "type": "string",
                                "const": "none"
                            }
                        },
                        "required": [
                            "source_edit_capability",
                            "verification_execution_capability",
                            "mutation_execution_capability",
                            "model_provider"
                        ],
                        "additionalProperties": false
                    },
                    "claim_boundary": { "type": "string" },
                    "limitations": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": [
                    "schema_version",
                    "workspace_state",
                    "root",
                    "configuration",
                    "trust",
                    "authority",
                    "claim_boundary",
                    "limitations"
                ],
                "additionalProperties": false
            },
            "session": {
                "type": "object",
                "properties": {
                    "schema_version": {
                        "type": "string",
                        "const": SESSION_SCHEMA_VERSION
                    },
                    "attempt_state": {
                        "type": "string",
                        "enum": ["no_snapshot", "in_flight", "completed", "failed"]
                    },
                    "current_attempt": {
                        "type": ["string", "null"],
                        "enum": ["in_flight", null]
                    },
                    "current_desired_input": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "const": "workspace_diff" },
                            "base": { "type": "string", "enum": ["default_branch", "explicit"] },
                            "base_revision": { "type": "string" },
                            "mode": { "type": "string", "const": "draft" },
                            "scope": { "type": "string", "const": "diff" }
                        },
                        "required": ["kind", "base", "mode", "scope"],
                        "additionalProperties": false
                    },
                    "scope": scope_disclosure_schema(),
                    "last_completed_snapshot": { "type": ["object", "null"] },
                    "last_known_good": { "type": ["object", "null"] },
                    "last_failure": { "type": ["object", "null"] },
                    "freshness": {
                        "type": "object",
                        "properties": {
                            "state": {
                                "type": "string",
                                "enum": [
                                    "current_at_last_refresh",
                                    "stale_after_failed_attempt",
                                    "none"
                                ]
                            },
                            "note": { "type": "string" }
                        },
                        "required": ["state", "note"],
                        "additionalProperties": false
                    },
                    "analysis_outcome": { "type": ["object", "null"] },
                    "profile": {
                        "type": "object",
                        "properties": {
                            "mode": { "type": "string", "const": "draft" },
                            "languages": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "project_config": {
                                "type": "string",
                                "enum": [
                                    "built_in_defaults",
                                    "detected_not_loaded",
                                    "loaded"
                                ]
                            },
                            "config_identity": {
                                "type": "string"
                            },
                            "support": {
                                "type": "string",
                                "enum": ["built_in_defaults", "project_config"]
                            }
                        },
                        "required": ["mode", "languages", "project_config", "support"],
                        "additionalProperties": false
                    },
                    "limitations": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": [
                    "schema_version",
                    "attempt_state",
                    "current_attempt",
                    "current_desired_input",
                    "scope",
                    "last_completed_snapshot",
                    "last_known_good",
                    "last_failure",
                    "freshness",
                    "analysis_outcome",
                    "profile",
                    "limitations"
                ],
                "additionalProperties": false
            },
            "mcp": {
                "type": "object",
                "properties": {
                    "transport": {
                        "type": "string",
                        "const": "stdio"
                    },
                    "tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 9,
                        "maxItems": 9
                    },
                    "resources": {
                        "type": "array",
                        "items": { "const": "ripr://workspace/status" },
                        "minItems": 1,
                        "maxItems": 1
                    },
                    "resource_templates": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 5,
                        "maxItems": 5
                    },
                    "bounds": {
                        "type": "object",
                        "properties": {
                            "max_message_bytes": {
                                "type": "integer",
                                "minimum": 1
                            },
                            "max_response_bytes": {
                                "type": "integer",
                                "minimum": 1
                            }
                        },
                        "required": ["max_message_bytes", "max_response_bytes"],
                        "additionalProperties": false
                    }
                },
                "required": ["transport", "tools", "resources", "resource_templates", "bounds"],
                "additionalProperties": false
            }
        },
        "required": ["schema_version", "workspace", "session", "mcp"],
        "additionalProperties": false
    })
}

fn refresh_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "schema_version": { "type": "string", "const": REFRESH_SCHEMA_VERSION },
                    "attempt": {
                        "type": "object",
                        "properties": {
                            "state": {
                                "type": "string",
                                "enum": ["no_snapshot", "in_flight", "completed", "failed"]
                            },
                            "failure": { "type": ["object", "null"] }
                        },
                        "required": ["state", "failure"],
                        "additionalProperties": false
                    },
                    "scope": scope_disclosure_schema(),
                    "requested_base": { "type": "string" },
                    "snapshot": { "type": ["object", "null"] },
                    "last_known_good": { "type": ["object", "null"] },
                    "claim_boundary": { "type": "string" },
                    "limitations": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["schema_version", "attempt", "scope", "snapshot", "last_known_good", "claim_boundary", "limitations"],
                "additionalProperties": false
            },
            typed_failure_document_schema(REFRESH_SCHEMA_VERSION)
        ]
    })
}

fn gap_list_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "schema_version": { "type": "string", "const": super::gaps::GAP_LIST_SCHEMA_VERSION },
                    "snapshot_id": { "type": "string" },
                    "requested_snapshot_id": { "type": ["string", "null"] },
                    "outcome": {
                        "type": "object",
                        "properties": {
                            "kind": {
                                "type": "string",
                                "enum": [
                                    "no_scope",
                                    "no_changed_lines",
                                    "no_behavioral_candidates",
                                    "complete_no_findings",
                                    "complete_with_findings",
                                    "partial_with_limitations",
                                    "unsupported_input",
                                    "analysis_failed"
                                ]
                            },
                            "label": { "type": "string" },
                            "limitations": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        },
                        "required": ["kind", "label", "limitations"],
                        "additionalProperties": false
                    },
                    "snapshot_profile_budget_identity": { "type": "string" },
                    "selection_basis_version": { "type": "string" },
                    "budget": {
                        "type": "object",
                        "properties": {
                            "max_items_per_workspace_response": { "type": "integer", "minimum": 1 },
                            "max_items_per_document": { "type": "integer", "minimum": 1 },
                            "max_serialized_bytes": { "type": "integer", "minimum": 1 },
                            "max_inline_detail_bytes": { "type": "integer", "minimum": 1 }
                        },
                        "required": [
                            "max_items_per_workspace_response",
                            "max_items_per_document",
                            "max_serialized_bytes",
                            "max_inline_detail_bytes"
                        ],
                        "additionalProperties": false
                    },
                    "total": { "type": "integer", "minimum": 0 },
                    "eligible": { "type": "integer", "minimum": 0 },
                    "selected": { "type": "integer", "minimum": 0 },
                    "filtered": { "type": "integer", "minimum": 0 },
                    "filters": {
                        "type": "object",
                        "properties": {
                            "file": { "type": ["string", "null"] },
                            "class": { "type": ["string", "null"] },
                            "language": { "type": ["string", "null"] },
                            "repair_ready": { "type": ["boolean", "null"] }
                        },
                        "required": ["file", "class", "language", "repair_ready"],
                        "additionalProperties": false
                    },
                    "omitted": { "type": "integer", "minimum": 0 },
                    "selected_bytes": { "type": "integer", "minimum": 0 },
                    "complete_bytes": { "type": "integer", "minimum": 0 },
                    "overflowed": { "type": "boolean" },
                    "overflow_reasons": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "enum": [
                                "document_item_limit",
                                "workspace_item_limit",
                                "serialized_byte_limit",
                                "inline_detail_limit"
                            ]
                        }
                    },
                    "items": { "type": "array" },
                    "page": {
                        "type": "object",
                        "properties": {
                            "offset": { "type": "integer", "minimum": 0 },
                            "limit": {
                                "type": ["integer", "null"],
                                "minimum": 1,
                                "description": "The caller-set page cap; null when the page byte-filled."
                            },
                            "returned": { "type": "integer", "minimum": 0 },
                            "has_more": { "type": "boolean" },
                            "next_offset": {
                                "type": ["integer", "null"],
                                "minimum": 0
                            }
                        },
                        "required": ["offset", "limit", "returned", "has_more", "next_offset"],
                        "additionalProperties": false
                    },
                    "omitted_items": { "type": "array" },
                    "continuation": { "type": "object" },
                    "claim_boundary": { "type": "string" },
                    "limitations": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": [
                    "schema_version",
                    "snapshot_id",
                    "requested_snapshot_id",
                    "outcome",
                    "snapshot_profile_budget_identity",
                    "selection_basis_version",
                    "budget",
                    "total",
                    "eligible",
                    "selected",
                    "filtered",
                    "filters",
                    "omitted",
                    "selected_bytes",
                    "complete_bytes",
                    "overflowed",
                    "overflow_reasons",
                    "items",
                    "page",
                    "omitted_items",
                    "continuation",
                    "claim_boundary",
                    "limitations"
                ],
                "additionalProperties": false
            },
            typed_failure_document_schema(super::gaps::GAP_LIST_SCHEMA_VERSION)
        ]
    })
}

fn gap_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "schema_version": { "type": "string", "const": GAP_SCHEMA_VERSION },
                    "snapshot_id": { "type": "string" },
                    "item": {
                        "type": "object",
                        "properties": {
                            "related_tests_total": { "type": "integer", "minimum": 0 }
                        },
                        "required": ["related_tests_total"]
                    },
                    "claim_boundary": { "type": "string" },
                    "limitations": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["schema_version", "snapshot_id", "item", "claim_boundary", "limitations"],
                "additionalProperties": false
            },
            typed_failure_document_schema(GAP_SCHEMA_VERSION)
        ]
    })
}

fn repair_packet_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "schema_version": { "type": "string", "const": REPAIR_PACKET_SCHEMA_VERSION },
                    "snapshot_id": { "type": "string" },
                    "requested_snapshot_id": { "type": ["string", "null"] },
                    "repair_packet_ready": { "type": "boolean" },
                    "ineligibility": { "type": "object" },
                    "recovery": { "type": "string" },
                    "item": { "type": "object" },
                    "attempt": { "type": ["object", "null"] },
                    "changed_behavior": { "type": ["object", "null"] },
                    "discriminator": { "type": ["string", "null"] },
                    "fix_site": { "type": ["object", "null"] },
                    "allowed_edit_surface": { "type": "array", "items": { "type": "string" } },
                    "must_not_change": { "type": "array", "items": { "type": "string" } },
                    "stop_conditions": { "type": "array", "items": { "type": "string" } },
                    "before_evidence": { "type": "object" },
                    "command_routes": { "type": "array" },
                    "command_route_limitation": { "type": "string" },
                    "limitations": { "type": "array", "items": { "type": "string" } },
                    "non_claims": { "type": "array", "items": { "type": "string" } },
                    "links": { "type": "object" },
                    "claim_boundary": { "type": "string" }
                },
                "required": [
                    "schema_version",
                    "snapshot_id",
                    "requested_snapshot_id",
                    "repair_packet_ready",
                    "attempt",
                    "claim_boundary"
                ],
                "additionalProperties": false
            },
            typed_failure_document_schema(REPAIR_PACKET_SCHEMA_VERSION)
        ]
    })
}

fn repair_attempt_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "schema_version": { "type": "string", "const": REPAIR_ATTEMPT_SCHEMA_VERSION },
                    "attempt_id": { "type": "string" },
                    "origin": { "type": "string", "enum": ["mcp_session", "durable_store"] },
                    "state": { "type": "string" },
                    "snapshot_id": { "type": ["string", "null"] },
                    "canonical_id": { "type": ["string", "null"] },
                    "root_identity": { "type": ["string", "null"] },
                    "repository_head": { "type": ["string", "null"] },
                    "seam_id": { "type": ["string", "null"] },
                    "producer_version": { "type": ["string", "null"] },
                    "created_unix_ms": { "type": ["integer", "null"] },
                    "packet": { "type": ["object", "null"] },
                    "artifacts": { "type": "array" },
                    "next_command": { "type": ["string", "object", "null"] },
                    "after": { "type": ["object", "null"] },
                    "terminal_receipt": { "type": ["string", "null"] },
                    "currentness": { "type": "object" },
                    "command_routes": { "type": "array" },
                    "limitations": { "type": "array", "items": { "type": "string" } },
                    "non_claims": { "type": "array", "items": { "type": "string" } },
                    "links": { "type": "object" },
                    "claim_boundary": { "type": "string" }
                },
                "required": [
                    "schema_version",
                    "attempt_id",
                    "origin",
                    "state",
                    "links",
                    "claim_boundary"
                ],
                "additionalProperties": false
            },
            typed_failure_document_schema(REPAIR_ATTEMPT_SCHEMA_VERSION)
        ]
    })
}

fn receipt_status_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "schema_version": { "type": "string", "const": RECEIPT_STATUS_SCHEMA_VERSION },
                    "receipt_id": { "type": "string" },
                    "status": {
                        "type": "string",
                        "enum": [
                            "awaiting_edit",
                            "after_pending",
                            "verification_pending",
                            "improved",
                            "closed",
                            "unchanged",
                            "regressed",
                            "limited",
                            "stale",
                            "invalid"
                        ]
                    },
                    "attempt": { "type": "object" },
                    "receipt": { "type": ["object", "null"] },
                    "currentness": { "type": "object" },
                    "limitations": { "type": "array", "items": { "type": "string" } },
                    "non_claims": { "type": "array", "items": { "type": "string" } },
                    "links": { "type": "object" },
                    "claim_boundary": { "type": "string" }
                },
                "required": [
                    "schema_version",
                    "receipt_id",
                    "status",
                    "attempt",
                    "receipt",
                    "currentness",
                    "claim_boundary"
                ],
                "additionalProperties": false
            },
            typed_failure_document_schema(RECEIPT_STATUS_SCHEMA_VERSION)
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_str<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{pointer} missing or not a string"))
    }

    /// #6021: the tool envelope used to serialize the document twice — a
    /// pretty-printed text copy plus `structuredContent` — so a listing the
    /// document guard approved at ~96 KB died on the wire at ~236 KB with
    /// `result_too_large` after every budget layer had passed. The envelope
    /// must carry the document once at full fidelity: compact text always,
    /// `structuredContent` in every success, and a typed failure whenever
    /// the complete envelope cannot fit — a success without
    /// `structuredContent` is not a shippable outcome for a tool that
    /// advertises an `outputSchema` (#7768).
    #[test]
    fn tool_result_carries_the_document_once_when_the_double_envelope_overflows()
    -> Result<(), String> {
        let rows = |count: usize| {
            json!({
                "schema_version": "ripr-mcp-gap-list-v3",
                "rows": (0..count)
                    .map(|index| {
                        json!({
                            "canonical_id": format!(
                                "gap:rust:src/module{index}.rs:owner:predicate_boundary:predicate:total{index}==10000"
                            ),
                            "class": "weakly_exposed",
                            "language": "rust",
                            "file": format!("src/module{index}.rs"),
                            "line": index + 1,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        };

        // Small documents keep the structured copy hosts consume.
        let small = tool_result(rows(4), super::super::MAX_RESPONSE_BYTES)
            .map_err(|error| error.to_string())?;
        let small_bytes = serde_json::to_vec(&small).map_err(|error| error.to_string())?;
        if small_bytes.len() > super::super::MAX_RESPONSE_BYTES {
            return Err(format!(
                "small envelope overflowed: {} bytes",
                small_bytes.len()
            ));
        }
        if small.get("structuredContent").is_none() {
            return Err("a small envelope must keep structuredContent".to_string());
        }
        let small_text = small
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or_else(|| "small envelope lost its text copy".to_string())?;
        if small_text.contains('\n') {
            return Err(
                "the text copy must be the compact serialization, not pretty print".to_string(),
            );
        }
        let reparsed: Value = serde_json::from_str(small_text)
            .map_err(|error| format!("text copy is not JSON: {error}"))?;
        if reparsed.get("schema_version") != Some(&json!("ripr-mcp-gap-list-v3")) {
            return Err(format!("text copy drifted from the document: {reparsed}"));
        }

        // A document the old document guard approved (~70 KB compact) but
        // whose doubled envelope (~164 KB) cannot fit the response bound:
        // it fails closed here. The former text-only fallback answered this
        // window with a success whose `structuredContent` was silently
        // absent — an undeclared break of the advertised output schema
        // (#7768). Only the pagers pre-shrink against this boundary
        // instead of refusing.
        let approved = rows(480);
        let approved_bytes = serde_json::to_string(&approved).map_err(|error| error.to_string())?;
        if approved_bytes.len() <= super::super::MAX_TOOL_DOCUMENT_BYTES
            || approved_bytes.len() >= super::super::MAX_RESPONSE_BYTES
        {
            return Err(format!(
                "fixture drifted: the window document must sit between the paging ceiling and the raw cap, got {} bytes",
                approved_bytes.len()
            ));
        }
        let refusal = match tool_result(approved, super::super::MAX_RESPONSE_BYTES) {
            Err(refusal) => refusal,
            Ok(envelope) => {
                return Err(format!(
                    "a window document must fail closed, not ship a success without structuredContent: {envelope}"
                ));
            }
        };
        if !refusal.contains("envelope budget")
            || !refusal.contains("structuredContent")
            || !refusal.contains(&super::super::MAX_RESPONSE_BYTES.to_string())
        {
            return Err(format!(
                "the window refusal must name the budget, the dropped field, and the bound: {refusal}"
            ));
        }

        // A document that cannot fit even as compact text fails closed
        // here, before any transport teardown.
        let oversized = rows(4000);
        let oversized_bytes =
            serde_json::to_string(&oversized).map_err(|error| error.to_string())?;
        if oversized_bytes.len() <= super::super::MAX_RESPONSE_BYTES {
            return Err(format!(
                "fixture drifted: the oversized document must exceed the raw cap, got {} bytes",
                oversized_bytes.len()
            ));
        }
        match tool_result(oversized, super::super::MAX_RESPONSE_BYTES) {
            Err(_) => Ok(()),
            Ok(envelope) => Err(format!(
                "an over-cap document must fail closed in the envelope builder, got {envelope}"
            )),
        }
    }

    /// #7768: the id-aware budget is a real boundary. The double-carry
    /// envelope carries the document twice (raw plus escaped text copy), so
    /// every padding byte moves it by exactly 2 and envelope sizes are
    /// even; an odd budget therefore makes budget-1 the last fitting size
    /// and budget+1 the first refused one — a true one-byte boundary.
    #[test]
    fn tool_result_fits_and_refuses_at_the_id_aware_envelope_budget() -> Result<(), String> {
        let document = |pad: usize| json!({ "rows": [{ "note": "x".repeat(pad) }] });
        let envelope_of =
            |pad: usize| -> Result<usize, String> { structured_envelope_bytes(&document(pad)) };
        let base = envelope_of(0)?;
        if base < 40 || base % 2 != 0 {
            return Err(format!("budget fixture base envelope drifted: {base}"));
        }
        let budget = base + 2 * 512 + 1;
        if envelope_of(512)? != budget - 1 || envelope_of(513)? != budget + 1 {
            return Err("budget fixture byte arithmetic drifted".into());
        }
        let overflow = |pad: usize, budget: usize| -> Result<bool, String> {
            structured_envelope_overflows(&document(pad), budget).map_err(|error| error.to_string())
        };
        if overflow(512, budget)? || !overflow(513, budget)? {
            return Err("the pager oracle must flip exactly between budget-1 and budget+1".into());
        }
        let under = tool_result(document(512), budget).map_err(|error| error.to_string())?;
        if under.get("structuredContent").is_none() {
            return Err("budget-1 must keep structuredContent".into());
        }
        // One byte past the budget the complete envelope cannot carry the
        // document with its structured copy (#7768): refuse typed instead
        // of silently dropping the copy the advertised output schema
        // promises.
        let refused = match tool_result(document(513), budget) {
            Err(detail) => detail,
            Ok(envelope) => {
                return Err(format!(
                    "budget+1 must fail closed, not ship a success without structuredContent: {envelope}"
                ));
            }
        };
        if !refused.contains("envelope budget") || !refused.contains(&budget.to_string()) {
            return Err(format!(
                "the budget+1 refusal must name the budget and the bound: {refused}"
            ));
        }
        // A document that cannot fit even as compact text fails closed with
        // the typed refusal detail naming both the budget and the bound.
        let detail = match tool_result(document(budget), budget) {
            Err(detail) => detail,
            Ok(envelope) => {
                return Err(format!(
                    "an over-budget text copy must fail closed: {envelope}"
                ));
            }
        };
        if !detail.contains("envelope budget")
            || !detail.contains(&budget.to_string())
            || !detail.contains(&super::super::MAX_RESPONSE_BYTES.to_string())
        {
            return Err(format!(
                "the compact-text refusal must name the budget and the bound: {detail}"
            ));
        }
        Ok(())
    }

    #[test]
    fn description_length_budgets_keep_session_context_lean() {
        // #9604 finding 33: every description below is injected into the
        // model's context for the whole session, so each surface carries a
        // byte budget. Both diet passes have landed; the budgets below are
        // the tightened per-surface ceilings.
        const TEMPLATE_BUDGET: usize = 475;
        const RESOURCE_BUDGET: usize = 650;
        const TOTAL_BUDGET_INTERIM: usize = 14900;
        for (name, text, budget) in [
            ("status_tool", STATUS_TOOL_DESCRIPTION, 1025),
            ("refresh_tool", REFRESH_TOOL_DESCRIPTION, 1450),
            ("list_gaps_tool", LIST_GAPS_TOOL_DESCRIPTION, 1075),
            ("get_gap_tool", GET_GAP_TOOL_DESCRIPTION, 975),
            ("prepare_repair_tool", PREPARE_REPAIR_TOOL_DESCRIPTION, 1390),
            (
                "get_repair_attempt_tool",
                GET_REPAIR_ATTEMPT_TOOL_DESCRIPTION,
                1285,
            ),
            (
                "get_receipt_status_tool",
                GET_RECEIPT_STATUS_TOOL_DESCRIPTION,
                1610,
            ),
            (
                "get_repair_card_tool",
                GET_REPAIR_CARD_TOOL_DESCRIPTION,
                1630,
            ),
            (
                "status_resource",
                STATUS_RESOURCE_DESCRIPTION,
                RESOURCE_BUDGET,
            ),
            (
                "snapshot_resource_template",
                SNAPSHOT_RESOURCE_TEMPLATE_DESCRIPTION,
                TEMPLATE_BUDGET,
            ),
            (
                "gap_resource_template",
                GAP_RESOURCE_TEMPLATE_DESCRIPTION,
                TEMPLATE_BUDGET,
            ),
            (
                "repair_attempt_resource_template",
                REPAIR_ATTEMPT_TEMPLATE_DESCRIPTION,
                TEMPLATE_BUDGET,
            ),
            (
                "receipt_resource_template",
                RECEIPT_TEMPLATE_DESCRIPTION,
                TEMPLATE_BUDGET,
            ),
            (
                "repair_card_resource_template",
                REPAIR_CARD_TEMPLATE_DESCRIPTION,
                TEMPLATE_BUDGET,
            ),
            ("instructions", INSTRUCTIONS, 1665),
        ] {
            assert!(
                text.len() <= budget,
                "{name} description is {} chars, over the {budget}-char budget",
                text.len()
            );
        }
        let total = STATUS_TOOL_DESCRIPTION.len()
            + REFRESH_TOOL_DESCRIPTION.len()
            + LIST_GAPS_TOOL_DESCRIPTION.len()
            + GET_GAP_TOOL_DESCRIPTION.len()
            + PREPARE_REPAIR_TOOL_DESCRIPTION.len()
            + GET_REPAIR_ATTEMPT_TOOL_DESCRIPTION.len()
            + GET_RECEIPT_STATUS_TOOL_DESCRIPTION.len()
            + GET_REPAIR_CARD_TOOL_DESCRIPTION.len()
            + STATUS_RESOURCE_DESCRIPTION.len()
            + SNAPSHOT_RESOURCE_TEMPLATE_DESCRIPTION.len()
            + GAP_RESOURCE_TEMPLATE_DESCRIPTION.len()
            + REPAIR_ATTEMPT_TEMPLATE_DESCRIPTION.len()
            + RECEIPT_TEMPLATE_DESCRIPTION.len()
            + REPAIR_CARD_TEMPLATE_DESCRIPTION.len()
            + INSTRUCTIONS.len();
        assert!(
            total <= TOTAL_BUDGET_INTERIM,
            "description total is {total} chars, over the {TOTAL_BUDGET_INTERIM}-char interim budget"
        );
    }

    #[test]
    fn status_tool_description_names_payload_scope_and_analysis_boundary() -> Result<(), String> {
        // The tool description is the only thing an LLM host sees before
        // spending a call. It must state positively what the document
        // contains, disavow the `ready`-means-analyzed misreading, name the
        // session contract, and point at the refresh route — or a future
        // edit can silently restore the exclusion-only framing.
        let tools = tools_list_result();
        let names = tools
            .pointer("/tools")
            .and_then(Value::as_array)
            .map(|tools| tools.len())
            .ok_or_else(|| "tools/list payload drifted".to_string())?;
        if names != 9 {
            return Err(format!(
                "expected the nine-tool slice surface, got {names} tools"
            ));
        }
        let description = json_str(&tools, "/tools/0/description")?;
        for required in [
            "repository-root discovery state",
            "source edit, verification execution, mutation execution, and model provider are all none",
            "not that analysis ran or that no issues were found",
            "session facts only: no gap evidence",
            "never edits source, runs tests or mutation, executes verification commands",
            "To run analysis, call ripr_refresh",
        ] {
            if !description.contains(required) {
                return Err(format!(
                    "status tool description lost payload boundary wording {required:?}: {description}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn status_output_schema_admits_success_and_typed_failure() -> Result<(), String> {
        // The status tool returns the success document or, when the
        // envelope exceeds the response bound, the shared typed failure
        // (#6291): schema-validating clients must accept both branches.
        let schema = status_output_schema();
        let branches = schema
            .pointer("/oneOf")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("status schema lost its oneOf branches: {schema}"))?;
        if branches.len() != 2 {
            return Err(format!(
                "status schema must branch success/failure, got {} branches",
                branches.len()
            ));
        }
        let required = branches[0]
            .pointer("/required")
            .and_then(Value::as_array)
            .ok_or_else(|| "status success branch lost its required fields".to_string())?;
        for field in ["schema_version", "workspace", "session", "mcp"] {
            if !required.iter().any(|entry| entry.as_str() == Some(field)) {
                return Err(format!("status success branch lost required {field:?}"));
            }
        }
        let expected_failure = typed_failure_document_schema(SESSION_SCHEMA_VERSION);
        if branches[1] != expected_failure {
            return Err(format!(
                "status failure branch drifted from the shared typed schema: {}",
                branches[1]
            ));
        }
        // The producer side: an oversized status renders exactly the
        // failure shape the second branch advertises.
        let failure = AttemptFailure::new(
            super::super::workspace::CODE_RESULT_TOO_LARGE,
            "tool response exceeds the bound",
            "read narrower evidence",
        );
        let envelope = tool_failure(&failure, SESSION_SCHEMA_VERSION)?;
        if envelope
            .pointer("/structuredContent/schema_version")
            .and_then(Value::as_str)
            != Some(SESSION_SCHEMA_VERSION)
        {
            return Err(format!("typed failure lost its schema version: {envelope}"));
        }
        if envelope
            .pointer("/structuredContent/failure/code")
            .and_then(Value::as_str)
            != Some(super::super::workspace::CODE_RESULT_TOO_LARGE)
        {
            return Err(format!("typed failure lost its code: {envelope}"));
        }
        Ok(())
    }

    #[test]
    fn tool_descriptions_state_positive_contracts_and_bounds() -> Result<(), String> {
        let tools = tools_list_result();
        let descriptions: [(&str, &str, &[&str]); 7] = [
            (
                "/tools/1/description",
                REFRESH_TOOL_NAME,
                &[
                    "shared check authority",
                    "never edits source, executes verification or mutation commands",
                    "cancelled attempt still commits as a completed snapshot when it finishes",
                    "only transport teardown abandons one before it commits",
                    "a superseded attempt is never committed",
                    "workspace_unavailable",
                    "unsupported_profile",
                ],
            ),
            (
                "/tools/2/description",
                LIST_GAPS_TOOL_NAME,
                &[
                    "deterministic bounded working set",
                    "never truncates silently",
                    "no business risk",
                    "stale_snapshot",
                    "no_snapshot",
                    "result_too_large",
                    "ripr_get_gap",
                ],
            ),
            (
                "/tools/3/description",
                GET_GAP_TOOL_NAME,
                &[
                    "complete bounded analysis evidence",
                    "prefer ripr_get_repair_card when the intent is to repair",
                    "repair_packet_ready now reports the committed producer repair-readiness facts",
                    "never authorizes an edit",
                    "item_not_found",
                    "ripr://gap/{canonical_id}",
                    "ripr check --committed --format json",
                    "related tests cap per finding",
                    "retrying ripr_get_gap cannot shrink the item",
                ],
            ),
            (
                "/tools/4/description",
                PREPARE_REPAIR_TOOL_NAME,
                &[
                    "create — or replay — one bounded in-memory repair transaction",
                    "never guesses a missing field and never creates a misleading attempt",
                    "never edits source, never launches a process",
                    "the external client's approval and sandbox policy remains authoritative",
                    "item_not_found",
                ],
            ),
            (
                "/tools/5/description",
                GET_REPAIR_ATTEMPT_TOOL_NAME,
                &[
                    "Read one repair transaction by its attempt identity",
                    "typed CommandSpec routes when the retained packet carries valid ones",
                    "attempt_invalid",
                    "attempt_not_found",
                    "Superseded tombstones older than the 64-entry session bound read attempt_not_found instead",
                    "ripr://repair-attempt/{attempt_id}",
                ],
            ),
            (
                "/tools/6/description",
                GET_RECEIPT_STATUS_TOOL_NAME,
                &[
                    "awaiting_edit, after_pending, verification_pending, improved, closed, unchanged, regressed, limited, stale, and invalid",
                    "RIPR performs no verification and issues no receipt",
                    "attempt-bound",
                    "the same reading as CLI status",
                    "complete changed stays open as limited",
                    "incomplete producer completeness stays limited",
                    "invalid producer status stays invalid",
                    "recorded stale or gap-mismatch lifecycle states retain stale or invalid",
                    "advisory regression preserves the trimmed, case-insensitive legacy static_movement.state fallback",
                    "receipt presence alone never reports closed",
                    "Static movement and focused runtime test execution remain separate evidence axes",
                    "ripr://receipt/{receipt_id}",
                ],
            ),
            (
                "/tools/7/description",
                GET_REPAIR_CARD_TOOL_NAME,
                &[
                    "prefer this card over ripr_get_gap when the agent intends to repair",
                    "same versioned repair_card.v1 document `ripr agent card` and the standard language server project",
                    "assembled by the shared application authority",
                    "The durable attempt store is re-read at card-read time",
                    "never a repair authorization",
                    "no_snapshot",
                    "seam_not_found",
                    "ripr://repair-card/{canonical_id}",
                ],
            ),
        ];
        for (pointer, name, required_words) in descriptions {
            let tool_name = json_str(&tools, pointer.replace("/description", "/name").as_str())?;
            if tool_name != name {
                return Err(format!("tool order drifted at {pointer}: {tool_name}"));
            }
            let description = json_str(&tools, pointer)?;
            for required in required_words {
                if !description.contains(required) {
                    return Err(format!(
                        "{name} description lost contract wording {required:?}: {description}"
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn status_resource_description_states_session_not_evidence() -> Result<(), String> {
        let resources = resources_list_result();
        let uri = json_str(&resources, "/resources/0/uri")?;
        if uri != STATUS_RESOURCE_URI {
            return Err(format!("unexpected resources/list payload: {resources}"));
        }
        let description = json_str(&resources, "/resources/0/description")?;
        for required in [
            "workspace discovery, authority, and session status",
            "No gap evidence detail is exposed here",
            "ripr_get_gap",
        ] {
            if !description.contains(required) {
                return Err(format!(
                    "status resource description lost boundary wording {required:?}: {description}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn resource_templates_expose_snapshot_and_gap_routes() -> Result<(), String> {
        let templates = resource_templates_list_result();
        let templates = templates
            .pointer("/resourceTemplates")
            .and_then(Value::as_array)
            .ok_or_else(|| "resourceTemplates payload drifted".to_string())?;
        if templates.len() != 5 {
            return Err(format!(
                "expected five resource templates, got {}",
                templates.len()
            ));
        }
        for (index, expected) in [
            (0, SNAPSHOT_RESOURCE_TEMPLATE),
            (1, GAP_RESOURCE_TEMPLATE),
            (2, REPAIR_ATTEMPT_TEMPLATE),
            (3, RECEIPT_TEMPLATE),
            (4, REPAIR_CARD_TEMPLATE),
        ] {
            let value = templates
                .get(index)
                .ok_or_else(|| format!("template {index} missing"))?;
            let uri = value
                .pointer("/uriTemplate")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("template {index} lost its uriTemplate"))?;
            if uri != expected {
                return Err(format!("template {index} drifted: {uri}"));
            }
            let description = value
                .pointer("/description")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("template {index} lost its description"))?;
            if description.is_empty() {
                return Err(format!("template {index} carries an empty description"));
            }
        }
        Ok(())
    }

    #[test]
    fn failure_vocabulary_covers_the_issue_contract() -> Result<(), String> {
        let codes = failure_vocabulary();
        for required in [
            "no_snapshot",
            "analysis_in_flight",
            "stale_snapshot",
            "workspace_unavailable",
            "unsupported_profile",
            "item_not_found",
            "result_too_large",
            "analysis_failed",
            "config_invalid",
            "workspace_ambiguous",
            "static_limitation",
            "cancelled",
            "superseded",
            "attempt_not_found",
            "attempt_invalid",
            "seam_not_found",
            "policy_omitted",
            "witness_unavailable",
            "identity_unnameable",
            "budget_overflow",
        ] {
            if !codes.contains(&required) {
                return Err(format!("failure vocabulary lost {required}: {codes:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn instructions_name_the_progressive_flow_and_the_cli_route() -> Result<(), String> {
        for required in [
            "ripr_workspace_status",
            "ripr_refresh",
            "ripr_list_gaps",
            "ripr_get_gap",
            "ripr_prepare_repair",
            "ripr_get_repair_attempt",
            "ripr_get_receipt_status",
            "ripr_get_repair_card",
            "ripr://gap/{canonical_id}",
            "ripr://repair-attempt/{attempt_id}",
            "ripr://receipt/{receipt_id}",
            "ripr://repair-card/{canonical_id}",
            "never edits source",
            "runs no tests or mutation",
            "launches no processes",
            "No document this server returns is a repair authorization",
            "approval and sandbox policy remain authoritative",
            "ripr check --format json",
        ] {
            if !INSTRUCTIONS.contains(required) {
                return Err(format!("instructions lost {required:?}: {INSTRUCTIONS}"));
            }
        }
        Ok(())
    }
}
