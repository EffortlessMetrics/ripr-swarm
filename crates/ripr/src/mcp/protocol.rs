use crate::workspace_status::{WorkspaceState, WorkspaceStatus};
use serde::Serialize;
use serde_json::{Value, json};

use super::gaps::{GAP_SCHEMA_VERSION, LIMITATION_SCHEMA_VERSION};
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
pub(super) const GET_TOP_LIMITATION_TOOL_NAME: &str = "ripr_get_top_limitation";
pub(super) const PREPARE_REPAIR_TOOL_NAME: &str = "ripr_prepare_repair";
pub(super) const GET_REPAIR_ATTEMPT_TOOL_NAME: &str = "ripr_get_repair_attempt";
pub(super) const GET_RECEIPT_STATUS_TOOL_NAME: &str = "ripr_get_receipt_status";
pub(super) const STATUS_RESOURCE_URI: &str = "ripr://workspace/status";
pub(super) const SNAPSHOT_RESOURCE_TEMPLATE: &str = "ripr://snapshot/{snapshot_id}";
pub(super) const GAP_RESOURCE_TEMPLATE: &str = "ripr://gap/{canonical_id}";
pub(super) const LIMITATION_RESOURCE_TEMPLATE: &str = "ripr://gap/{canonical_id}/limitation";
/// The typed-failure vocabulary every evidence tool can return. Codes a
/// tool cannot reach in this slice stay named (reserved) so the wire
/// contract is stable when the owning slice lands; each tool description
/// says which codes it can actually return.
pub(super) fn failure_vocabulary() -> Vec<&'static str> {
    let mut codes = vec![
        "workspace_unavailable",
        "analysis_failed",
        "unsupported_profile",
        super::workspace::CODE_CONFIG_INVALID,
        CODE_NO_SNAPSHOT,
        "analysis_in_flight",
        "stale_snapshot",
        "item_not_found",
        super::workspace::CODE_INVALID_ARGUMENT,
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
pub(super) const INSTRUCTIONS: &str = "Flow: call `ripr_workspace_status` for root discovery, authority, and session facts; `ripr_refresh` defaults to the committed default-branch diff and commits a completed snapshot. `ripr_list_gaps` lists the bounded working set; read evidence with `ripr_get_gap` or ripr://gap/{canonical_id}; for repair prefer `ripr_get_repair_card` (ripr://repair-card/{canonical_id}). With established repair-readiness facts, `ripr_prepare_repair` creates (or replays) one bounded in-memory repair transaction; read it with `ripr_get_repair_attempt` (ripr://repair-attempt/{attempt_id}), inspect receipts with `ripr_get_receipt_status` (ripr://receipt/{receipt_id}), and list every durable attempt with `ripr_list_repair_attempts`. RIPR is a static analyzer that asks whether the current tests would notice if the behavior changed in a diff were wrong. This MCP server exposes one read-only workspace session: it never edits source, executes nothing, runs no tests or mutation, and launches no processes; it names CLI routes without invoking them. No document this server returns is a repair authorization: the external client's approval and sandbox policy remain authoritative. By default: committed input only; staged and unstaged tracked edits and untracked files are outside. Opt in with `ripr_refresh` `source: \"worktree\"` to include saved staged and unstaged tracked edits; unsaved buffers and untracked files remain outside, and worktree repair cards/readiness are withheld. `ripr check` defaults to the working tree when staged or unstaged tracked edits exist; shared check authority does not mean identical input. `ripr check --committed` reads committed input; `ripr check --worktree --format json` adds staged and unstaged tracked edits only. Untracked files need staging/intent-to-add (`git add -N <paths>`) before `--worktree`, or explicit `--diff`.";

const STATUS_TOOL_DESCRIPTION: &str = "Report the RIPR workspace and session state — call this first in a session: it is the root-discovery, authority, and session read. Covers repository-root discovery state (validated or unavailable; root error detail names why), launch-trust and authority facts (source edit, verification execution, mutation execution, and model provider are all none), and a session block: desired input, attempt state (no_snapshot, in_flight, completed, failed), last completed snapshot identity, last-known-good, freshness, typed AnalysisOutcome, and profile facts (project_config loaded, built-in defaults, or detected_not_loaded). `workspace_state: ready` means only that a repository root was discovered — not that analysis ran or that no issues were found. This tool returns session facts only: no gap evidence. It never edits source, runs tests or mutation, executes verification commands. To run analysis, call ripr_refresh.";

const REFRESH_TOOL_DESCRIPTION: &str = "Run one bounded static analysis of the default-branch diff (optional `base` selects another revision) through shared check authority; commit its completed session snapshot. `source: \"worktree\"` opts into saved tracked edits, with no repair cards. Call this when the session has no current snapshot or the selected input changed since the last one. The result has fresh_for_working_tree and a verify route (re-check after editing tests). Blocks until: completed (snapshot identity returned), failed (isError; typed code, detail, recovery; last-known-good kept), in_flight (another attempt is running — wait and retry), or workspace_unavailable (restart with `--root <repository>`). An unresolvable default branch fails with analysis_failed naming candidate branches; retry with `base`. A `base` starting with `-` or naming no commit fails closed; omitting `base` returns to the default branch. A cancelled attempt still commits as a completed snapshot when it finishes and only transport teardown abandons one before it commits; a superseded attempt is never committed. Unsupported profiles fail closed with unsupported_profile. It never edits source, executes verification or mutation commands, or prepares a repair. By default: committed input only; saved tracked edits require `source: \"worktree\"`; unsaved buffers and untracked files remain outside; see INSTRUCTIONS for the scope routes.";

const LIST_GAPS_TOOL_DESCRIPTION: &str = "Return the deterministic bounded working set of canonical items for the current completed snapshot (optional snapshot_id must match, else stale_snapshot). Call this after ripr_refresh to choose which items merit a detail read. The response carries counts, omitted identities with reasons, the selection basis, and one small summary per selected item (fields in the outputSchema). Selection is the shared CLI/LSP budget authority: MCP does not re-rank, never truncates silently, and infers no business risk; overflow is disclosed with reasons. Optional file, class, language, and repair_ready filters narrow the stored selection conjunctively before paging; offset and limit page the narrowed set in snapshot order, disclosed through page.has_more and page.next_offset. Fails closed with no_snapshot before the first successful ripr_refresh, workspace_unavailable on an unavailable root (restart with --root <repository>), invalid_argument, and result_too_large on an over-bound page. Per-item detail: ripr_get_gap or ripr://gap/{canonical_id}.";

const GET_GAP_TOOL_DESCRIPTION: &str = "Return one canonical item's complete bounded analysis evidence from the current completed snapshot (optional snapshot_id must match or the call fails closed with stale_snapshot). This is the read for analysis evidence when no repair intent exists — prefer ripr_get_repair_card when the intent is to repair; this evidence never authorizes an edit. repair_packet_ready now reports the committed producer repair-readiness facts. Unknown ids fail closed with item_not_found; a document over the response bound fails with result_too_large — follow the failure's captured-root/base/source CLI recovery outside MCP (a bounded summary, not immutable snapshot replay); retrying ripr_get_gap cannot shrink the item. Fails closed with no_snapshot before the first successful ripr_refresh and workspace_unavailable on an unavailable root. The resource ripr://gap/{canonical_id} returns the same document.";

const GET_TOP_LIMITATION_TOOL_DESCRIPTION: &str = "Return the earliest committed producer limitation that prevents a safe repair route for one canonical item of the current completed snapshot (optional snapshot_id must match, else stale_snapshot): kind, stage, reason, recovery, restart identity, retryable, and the stronger claim that stays prohibited. Producer static limits outrank readiness gates; an item with none returns kind no_limitation_established, never null. Reads committed facts only, with no analysis rerun. Fails closed with item_not_found, no_snapshot, and workspace_unavailable. The resource ripr://gap/{canonical_id}/limitation returns the same document.";

const PREPARE_REPAIR_TOOL_DESCRIPTION: &str = "When every committed producer repair-readiness gate reads established, create — or replay — one bounded in-memory repair transaction for one canonical item of the current completed snapshot. Call this when the agent intends to repair an item from ripr_list_gaps; the readiness gates are on that item's evidence. Inputs: canonical_id (required, from ripr_list_gaps) and optional snapshot_id, which must match the current snapshot or the call fails closed with stale_snapshot. The outputSchema enumerates the complete document. Repeating the call for the same snapshot, item, and root replays the identical document — never a second transaction. When a gate is not established the tool returns isError: true, repair_packet_ready: false, the typed ineligibility reason, a next step and attempt: null — it never guesses a missing field and never creates a misleading attempt. This tool never edits source, never launches a process, and never executes verification or mutation commands; the external client's approval and sandbox policy remains authoritative. Fails closed with no_snapshot before the first successful ripr_refresh, workspace_unavailable on an unavailable root, and item_not_found on unknown canonical ids.";

const GET_REPAIR_ATTEMPT_TOOL_DESCRIPTION: &str = "Read one repair transaction by its attempt identity, without executing anything it names. Call this with an attempt id from ripr_prepare_repair or the durable store, before acting on it. Session transactions from ripr_prepare_repair answer first: immutable packet, awaiting_edit state, snapshot and root identity binding, resource links. Otherwise the shared repair-attempt authority reads the durable attempt store of this root: a valid manifest projects state, repository head, seam identity, artifact digest bindings, follow-up command display string, and typed CommandSpec routes when the retained packet carries valid ones — the human display is never execution authority and shell_required or manual modes are visibly non-direct; the host-local root path is never projected. A manifest that fails canonical validation fails closed with attempt_invalid; an unknown identity fails closed with attempt_not_found. An attempt bound to a stale snapshot fails closed with the reserved superseded state and the current snapshot identity. Superseded tombstones older than the 64-entry session bound read attempt_not_found instead. Never edits source, launches a process, or executes verification or mutation commands. The resource ripr://repair-attempt/{attempt_id} returns the same document.";

const GET_RECEIPT_STATUS_TOOL_DESCRIPTION: &str = "Read the current receipt state for one attempt identity (receipt ids are attempt-bound: one retained receipt per durable attempt). Call this after external verification finished to learn whether the attempt improved, regressed, or stayed unchanged — and before trusting an old receipt that the snapshot may have outrun. The status vocabulary is awaiting_edit, after_pending, verification_pending, improved, closed, unchanged, regressed, limited, stale, and invalid. Session transactions report awaiting_edit with an explicit null receipt: RIPR performs no verification and issues no receipt. Durable attempts report their producer state mapped onto the same vocabulary; the terminal mapping is owned by the same reading as CLI status and is documented on the outputSchema status field. A finished digest-bound terminal receipt projects the receipt document with exact byte bindings — and receipt presence alone never reports closed; a manifest or receipt that fails canonical validation reports invalid or attempt_invalid rather than a reconstructed state. Status binds exact before/after/verify bytes, repository identity, candidate item, and currentness, re-validated on every read — never mtime or latest-file convention. Static movement and focused runtime test execution remain separate evidence axes — this document reports static receipt state only. This read edits nothing and executes nothing. The resource ripr://receipt/{receipt_id} returns the same document.";

const LIMITATION_RESOURCE_TEMPLATE_DESCRIPTION: &str = "One canonical item's earliest producer limitation, the same document ripr_get_top_limitation returns: typed kind, stage, reason, recovery, retryable, restart identity, and the prohibited stronger claim. The item must exist in the current completed snapshot (else item_not_found).";

const STATUS_RESOURCE_DESCRIPTION: &str = "Bounded, read-only workspace discovery, authority, and session status: repository-root discovery state, configuration presence, launch-trust and authority facts, and the current analysis session (attempt state, last completed snapshot identity, typed AnalysisOutcome, profile facts). No gap evidence detail is exposed here; run ripr_refresh, then read items through ripr_get_gap. On an unavailable root the payload carries the same recovery sentence as the ripr_workspace_status tool result as a second text content entry, naming the cause and the restart route (`ripr mcp --stdio --root <repository>`).";

const SNAPSHOT_RESOURCE_TEMPLATE_DESCRIPTION: &str = "Bounded evidence for one completed snapshot: snapshot identity, typed AnalysisOutcome, the canonical item index (identities and locations, not evidence), and the stored selection summary. One item's complete evidence: ripr_get_gap or ripr://gap/{canonical_id}. Unknown or superseded ids fail closed with no_snapshot or stale_snapshot; an unavailable root fails closed with workspace_unavailable — restart the server with `ripr mcp --stdio --root <repository>`.";

const GAP_RESOURCE_TEMPLATE_DESCRIPTION: &str = "One canonical item's complete evidence — changed behavior, causal attribution, discriminator availability, related tests, and the committed producer repair-readiness block; the same document ripr_get_gap returns. The item must exist in the current completed snapshot (else item_not_found). A document over the response bound fails with result_too_large — follow the failure's captured-root/base/source CLI recovery outside MCP; retrying this resource cannot shrink the item.";

const REPAIR_ATTEMPT_TEMPLATE_DESCRIPTION: &str = "One repair transaction, as returned by ripr_get_repair_attempt: a session transaction from ripr_prepare_repair or a durable manifest with digest bindings and typed CommandSpec routes when valid. Unknown identities fail closed (attempt_not_found), invalid manifests (attempt_invalid), superseded-snapshot bindings (superseded), and tombstones past the 64-entry bound (attempt_not_found). The read never edits source or executes anything the attempt names.";

const RECEIPT_TEMPLATE_DESCRIPTION: &str = "The receipt status document for one attempt identity, identical to the ripr_get_receipt_status tool result: the status vocabulary (awaiting_edit, after_pending, verification_pending, improved, closed, unchanged, regressed, limited, stale, invalid), the digest-bound receipt document when a durable attempt retained one, and the currentness basis. Receipt issuance is external authority — RIPR performs no verification and executes nothing on this read.";

const REPAIR_CARD_TEMPLATE_DESCRIPTION: &str = "One canonical item's bounded repair card, the same document ripr_get_repair_card returns: the repair_card.v1 document `ripr agent card` and the language server project assemble from the committed snapshot by the shared authority. The card names the seam, witness, fix instruction, edit cage, attempt state, and typed next action. The item must exist in the current snapshot with an owner-discriminated seam binding (else item_not_found, seam_not_found, or policy_omitted).";

const GET_REPAIR_CARD_TOOL_DESCRIPTION: &str = "Project the bounded repair card for one canonical item of the current completed snapshot: the same versioned repair_card.v1 document `ripr agent card` and the standard language server project, assembled by the shared application authority. Route disambiguation: prefer this card over ripr_get_gap when the agent intends to repair; ripr_get_gap is the lighter read when only analysis evidence is wanted. Inputs: canonical_id (required, from ripr_list_gaps) and optional snapshot_id, which must match the current snapshot or the call fails closed with stale_snapshot. The document carries the verbatim card plus the claim boundary and resource links. The durable attempt store is re-read at card-read time, so the attempt block matches `ripr agent status` at this moment; in-memory session transactions never ride a card. The card edits nothing, executes nothing, and is never a repair authorization: the external client's approval and sandbox policy remains authoritative for the command the next action names, and the display binds the portable root `.` (never the host-local path). Fails closed with no_snapshot before the first successful ripr_refresh and seam_not_found for an item with no seam binding. The resource ripr://repair-card/{canonical_id} returns the same document.";

/// Environment variable that selects [`ToolsListMode`] for one server process.
pub(super) const TOOLS_LIST_ENV: &str = "RIPR_MCP_TOOLS_LIST";

/// Whether `tools/list` advertises each tool's `outputSchema` (#11621).
///
/// `Full` (the default) keeps every `outputSchema`, about two thirds of the
/// payload, so a host that validates results against them keeps working.
/// `Slim` is an opt-in for hosts that never read them: the schemas are
/// omitted, `structuredContent` is unchanged and still carries
/// `schema_version`, and the document shapes stay in `docs/interop/mcp.md`.
/// An unrecognized value keeps the default: a typo must never remove a
/// contract a validating host relies on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ToolsListMode {
    #[default]
    Full,
    Slim,
}

impl ToolsListMode {
    pub(super) fn from_env_value(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("slim") => Self::Slim,
            _ => Self::Full,
        }
    }

    pub(super) fn from_env() -> Self {
        Self::from_env_value(std::env::var(TOOLS_LIST_ENV).ok().as_deref())
    }
}

#[cfg(test)]
pub(super) fn tools_list_result() -> Value {
    tools_list_result_for(ToolsListMode::Full)
}

pub(super) fn tools_list_result_for(mode: ToolsListMode) -> Value {
    let mut list = full_tools_list_result();
    if mode == ToolsListMode::Slim
        && let Some(tools) = list.get_mut("tools").and_then(Value::as_array_mut)
    {
        for tool in tools {
            if let Some(descriptor) = tool.as_object_mut() {
                descriptor.remove("outputSchema");
            }
        }
    }
    list
}

fn full_tools_list_result() -> Value {
    // #9852 context-sink budget: this payload rides every session start, so
    // it carries a pinned byte ceiling (the `tools_list_byte_budget` guard
    // below) the same way descriptions carry byte budgets (#9818/#9848).
    // The obvious next lever — factoring repeated outputSchema subschemas
    // through `$defs`/`$ref` — was evaluated and rejected on arithmetic,
    // not taste. Every advertised `outputSchema` is a standalone schema
    // document: a host validates a tool result against that value alone, so
    // a ref may only target `#/$defs/...` inside the same `outputSchema`,
    // and the wire's big duplication is cross-document — the ~800-byte
    // typed-failure branch repeats in all nine tools (~7.3 KB) with only
    // the `schema_version` const differing. Factoring that across tools
    // would dangle for standalone validators and break the "validate
    // identically" contract. Within one document, the only repeated
    // subschemas are 40-68-byte leaves occurring 2-5 times, and moving one
    // copy into `$defs` plus `$ref`s costs more bytes than the replaced
    // copies at every such size (best case ~75 bytes across all nine
    // schemas, before overlap exclusion). What remains honest is dropping
    // the `annotations.title` duplicate of each top-level `title` (done in
    // the descriptors below) and pinning the ceiling so the sink cannot
    // grow back unnoticed.
    json!({"tools": [
        status_tool_descriptor(),
        refresh_tool_descriptor(),
        list_gaps_tool_descriptor(),
        get_gap_tool_descriptor(),
        get_top_limitation_tool_descriptor(),
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
            "uriTemplate": LIMITATION_RESOURCE_TEMPLATE,
            "name": "ripr-gap-limitation",
            "title": "RIPR gap limitation",
            "description": LIMITATION_RESOURCE_TEMPLATE_DESCRIPTION,
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
    tools: [&'static str; 10],
    resources: [&'static str; 1],
    resource_templates: [&'static str; 6],
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
                GET_TOP_LIMITATION_TOOL_NAME,
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
                LIMITATION_RESOURCE_TEMPLATE,
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
    text_mode: TextContentMode,
) -> Result<Value, String> {
    let document = status_document(
        status,
        session,
        profile,
        max_message_bytes,
        max_response_bytes,
    );
    let mut envelope = tool_result_with(document, max_envelope_bytes, text_mode)?;
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
#[cfg(test)]
pub(super) fn tool_result(document: Value, max_envelope_bytes: usize) -> Result<Value, String> {
    tool_result_with(document, max_envelope_bytes, TextContentMode::Full)
}

/// Environment variable that selects [`TextContentMode`] for one server
/// process (set it in the MCP host's server `env`).
pub(super) const TEXT_CONTENT_ENV: &str = "RIPR_MCP_TEXT_CONTENT";

/// What `content[0].text` carries on a successful tool result. The complete
/// document is always in `structuredContent`; this only selects the text
/// companion the MCP specification asks for on behalf of hosts that do not
/// read structured results (#9360).
///
/// `Full` (the default) is the compact JSON serialization of the document, so
/// a host that reads only text content loses nothing. `Summary` is an opt-in
/// token saver for hosts known to read `structuredContent`: a short digest
/// that states plainly it is not a copy and how to get the full text. The
/// default is not flipped because a host that feeds only text content to its
/// model would silently lose the result, which is a worse failure than the
/// duplicated tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum TextContentMode {
    #[default]
    Full,
    Summary,
    /// Opt-in like `Summary`, but useful on its own for a gap list: one line
    /// per item (`class file:line canonical_id repair_ready=...`) so a host
    /// that surfaces only text still sees which gap is which. Documents that
    /// are not gap lists fall back to the `Summary` digest (#11621).
    Brief,
}

impl TextContentMode {
    /// `summary` or `brief` (ASCII case-insensitive, surrounding whitespace
    /// ignored) select [`TextContentMode::Summary`] or
    /// [`TextContentMode::Brief`]. Every other value, including a
    /// typo, keeps the lossless default: an unrecognized spelling must never
    /// shrink what a text-only host receives.
    pub(super) fn from_env_value(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("summary") => Self::Summary,
            Some(value) if value.eq_ignore_ascii_case("brief") => Self::Brief,
            _ => Self::Full,
        }
    }

    pub(super) fn from_env() -> Self {
        Self::from_env_value(std::env::var(TEXT_CONTENT_ENV).ok().as_deref())
    }
}

/// [`tool_result`] with an explicit text companion. The size guard always
/// measures the complete double-carry envelope, so a document is admitted or
/// refused identically in both modes (paging and `result_too_large` do not
/// depend on the mode); `Summary` only replaces `content[0].text` afterward.
pub(super) fn tool_result_with(
    document: Value,
    max_envelope_bytes: usize,
    mode: TextContentMode,
) -> Result<Value, String> {
    tool_result_flagged(document, max_envelope_bytes, mode, false)
}

/// [`tool_result_with`] with an explicit `isError`. A refusal or a failed
/// attempt keeps its typed document in `structuredContent` (the advertised
/// output schema still describes it) but must not read as a success to a
/// host that keys on `isError` (agent-experience MCP audit findings 4a, 6).
pub(super) fn tool_result_flagged(
    document: Value,
    max_envelope_bytes: usize,
    mode: TextContentMode,
    is_error: bool,
) -> Result<Value, String> {
    let text = serde_json::to_string(&document)
        .map_err(|error| format!("render tool document: {error}"))?;
    let mut structured = json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": document,
        "isError": is_error,
    });
    let bytes = envelope_bytes(&structured)?;
    if bytes > max_envelope_bytes {
        return Err(format!(
            "tool document's complete envelope renders {bytes} bytes against the {max_envelope_bytes}-byte envelope budget ({}-byte response bound minus the request id and outer reserve); a success without structuredContent would break the advertised output schema",
            super::MAX_RESPONSE_BYTES
        ));
    }
    if mode != TextContentMode::Full {
        let summary = structured
            .get("structuredContent")
            .map(|document| match mode {
                TextContentMode::Brief => brief_text(document),
                _ => summary_text(document),
            })
            .ok_or_else(|| "tool envelope lost structuredContent".to_string())?;
        if let Some(slot) = structured.pointer_mut("/content/0/text") {
            *slot = Value::String(summary);
        }
    }
    Ok(structured)
}

/// The opt-in `brief` text: for a gap list, a header with the paging facts
/// and one line per item; for every other document, the `Summary` digest.
/// Every value is a projection of `structuredContent`, with control
/// characters escaped so a document string cannot forge a line.
fn brief_text(document: &Value) -> String {
    let is_gap_list = document.get("schema_version").and_then(Value::as_str)
        == Some(super::gaps::GAP_LIST_SCHEMA_VERSION);
    let Some(items) = document
        .get("items")
        .and_then(Value::as_array)
        .filter(|_| is_gap_list)
    else {
        return summary_text(document);
    };
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map_or_else(|| "?".to_string(), escape_summary)
    };
    let count = |key: &str| {
        document
            .get(key)
            .and_then(Value::as_u64)
            .map_or_else(|| "?".to_string(), |number| number.to_string())
    };
    let mut lines = vec![format!(
        "ripr gaps: {} shown, {} eligible, {} omitted. This text is a brief, not a copy: the complete result is `structuredContent`; start the server with {TEXT_CONTENT_ENV}=full for the full JSON here.",
        items.len(),
        count("eligible"),
        count("omitted"),
    )];
    for item in items {
        let line = item
            .get("line")
            .and_then(Value::as_u64)
            .map_or_else(|| "?".to_string(), |number| number.to_string());
        let readiness = if item.get("repair_packet_ready").and_then(Value::as_bool) == Some(true) {
            "true".to_string()
        } else {
            format!("false:{}", text(item, "repair_ineligibility"))
        };
        lines.push(format!(
            "{} {}:{} {} repair_ready={}",
            text(item, "class"),
            text(item, "file"),
            line,
            text(item, "canonical_id"),
            readiness
        ));
    }
    if let Some(next) = document
        .pointer("/page/next_offset")
        .and_then(Value::as_u64)
    {
        lines.push(format!(
            "more: call ripr_list_gaps with offset={next} (and the same filters)"
        ));
    }
    lines.join("\n")
}

const SUMMARY_STRING_CHARS: usize = 200;
const SUMMARY_LISTED_IDS: usize = 5;
const SUMMARY_LISTED_FIELDS: usize = 12;

/// The opt-in text digest of a document: a header that says this is not the
/// full result, then one line per top-level field — scalars verbatim (long
/// strings truncated with a marker), arrays as counts (plus the first few
/// `canonical_id`s so a text-only agent can still call `ripr_get_gap`), and
/// objects as their field names. No fact is invented or reworded; every line
/// is a projection of the structured document.
fn summary_text(document: &Value) -> String {
    let mut lines = Vec::new();
    let schema = document
        .get("schema_version")
        .and_then(Value::as_str)
        .map_or_else(|| "unknown schema".to_string(), escape_summary);
    lines.push(format!(
        "ripr summary ({schema}). This text is a digest, not a copy: the complete result is `structuredContent`. A host that reads only text content should start the server with {TEXT_CONTENT_ENV}=full to receive the full JSON here."
    ));
    let Some(object) = document.as_object() else {
        return lines.join("\n");
    };
    for (raw_key, value) in object {
        if raw_key == "schema_version" {
            continue;
        }
        let key = escape_summary(raw_key);
        lines.push(match value {
            Value::Null => format!("{key}: null"),
            Value::Bool(flag) => format!("{key}: {flag}"),
            Value::Number(number) => format!("{key}: {number}"),
            Value::String(text) => format!("{key}: {}", clip_summary_string(text)),
            Value::Array(entries) => {
                let ids: Vec<String> = entries
                    .iter()
                    .filter_map(|entry| entry.get("canonical_id").and_then(Value::as_str))
                    .take(SUMMARY_LISTED_IDS)
                    .map(escape_summary)
                    .collect();
                if ids.is_empty() {
                    format!("{key}: {} entries", entries.len())
                } else {
                    format!(
                        "{key}: {} entries; first canonical_id values: {}",
                        entries.len(),
                        ids.join(", ")
                    )
                }
            }
            Value::Object(fields) => {
                let names: Vec<String> = fields
                    .keys()
                    .map(|name| escape_summary(name))
                    .take(SUMMARY_LISTED_FIELDS)
                    .collect();
                let more = fields.len().saturating_sub(names.len());
                if more == 0 {
                    format!("{key}: object with fields {}", names.join(", "))
                } else {
                    format!(
                        "{key}: object with fields {} and {more} more",
                        names.join(", ")
                    )
                }
            }
        });
    }
    lines.join("\n")
}

/// Control characters (newlines above all) are escaped so a document string
/// can never start a new digest line and forge a `key: value` entry.
fn escape_summary(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            let escaped: Vec<char> = if character.is_control() {
                character.escape_default().collect()
            } else {
                vec![character]
            };
            escaped
        })
        .collect()
}

fn clip_summary_string(text: &str) -> String {
    if text.chars().count() <= SUMMARY_STRING_CHARS {
        return escape_summary(text);
    }
    let clipped: String = text.chars().take(SUMMARY_STRING_CHARS).collect();
    format!(
        "{}... (truncated; full text in structuredContent)",
        escape_summary(&clipped)
    )
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
                },
                "source": {
                    "type": "string",
                    "enum": ["committed", "worktree"],
                    "description": "Input to analyze. Omit for `committed` (the committed diff). `worktree` analyzes saved staged and unstaged tracked edits; repair cards and readiness are withheld."
                }
            },
            "additionalProperties": false
        },
        "outputSchema": refresh_output_schema(),
        "annotations": {
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
                "omitted_offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Start index within the omitted identities, which page on their own axis (bounded by count and bytes per response). Follow continuation.next_omitted_page. Default 0."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": crate::lsp::diagnostic_budget::DEFAULT_MAX_ITEMS_PER_WORKSPACE_RESPONSE,
                    "description": "Maximum narrowed-item summaries to return in this page (the working-set bound; a larger value is refused as invalid_argument)."
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
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn get_top_limitation_tool_descriptor() -> Value {
    json!({
        "name": GET_TOP_LIMITATION_TOOL_NAME,
        "title": "RIPR top limitation",
        "description": GET_TOP_LIMITATION_TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "canonical_id": { "type": "string", "minLength": 1 },
                "snapshot_id": { "type": "string" }
            },
            "required": ["canonical_id"],
            "additionalProperties": false
        },
        "outputSchema": limitation_output_schema(),
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn limitation_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "schema_version": { "type": "string", "const": LIMITATION_SCHEMA_VERSION },
                    "snapshot_id": { "type": "string" },
                    "canonical_id": { "type": "string" },
                    "class": { "type": "string" },
                    "limitation": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string" },
                            "stage": {
                                "type": "string",
                                "enum": ["producer_static_limit", "seam_admission", "repair_readiness", "none"]
                            },
                            "reason": { "type": "string" },
                            "recovery": { "type": "string" },
                            "retryable": { "type": "boolean" },
                            "prohibited_claim": { "type": "string" },
                            "restart": {
                                "type": "object",
                                "properties": {
                                    "snapshot_id": { "type": "string" },
                                    "canonical_id": { "type": "string" },
                                    "route": { "type": "string" }
                                },
                                "required": ["snapshot_id", "canonical_id", "route"]
                            }
                        },
                        "required": ["kind", "stage", "reason", "recovery", "retryable", "prohibited_claim", "restart"]
                    },
                    "links": { "type": "object" },
                    "claim_boundary": { "type": "string" },
                    "limitations": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["schema_version", "snapshot_id", "canonical_id", "limitation", "claim_boundary", "limitations"],
                "additionalProperties": false
            },
            typed_failure_document_schema(LIMITATION_SCHEMA_VERSION)
        ]
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
                    "finding_guidance": { "type": "object" },
                    "mcp_detail_routes": { "type": "array", "items": { "type": "object" } },
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
                "pattern": "^(committed (default-branch diff|diff against base .+)|saved tracked-worktree diff against (the default branch|base .+))$"
            },
            "uncommitted_edits": {
                "type": "string",
                "enum": ["outside this analysis", "saved tracked edits included"]
            },
            "note": { "type": "string" },
            "consumed_source_commitment": { "type": "string", "enum": ["not_established", "established"] },
            "repair_cards": { "type": "string", "enum": ["withheld", "available"] },
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

pub(super) fn status_success_schema() -> Value {
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
                            "source": { "type": "string", "const": "worktree" },
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
                    "language_completeness": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "language": { "type": "string" },
                                "status": {
                                    "type": "string",
                                    "enum": [
                                        "complete",
                                        "complete_zero",
                                        "partial",
                                        "unavailable",
                                        "invalid",
                                        "failed",
                                        "unsupported",
                                        "not_selected",
                                        "no_current_scope"
                                    ]
                                }
                            },
                            "required": ["language", "status"],
                            "additionalProperties": false
                        }
                    },
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
                        "minItems": 10,
                        "maxItems": 10
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
                        "minItems": 6,
                        "maxItems": 6
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
                    "fresh_for_working_tree": { "type": ["boolean", "null"] },
                    "verify": { "type": "object" },
                    "requested_base": { "type": "string" },
                    "empty_scope_hint": {
                        "type": "object",
                        "properties": {
                            "why": { "type": "string" },
                            "pass_base": { "type": "string" },
                            "pass_source": { "type": "string" }
                        },
                        "required": ["why", "pass_base", "pass_source"],
                        "additionalProperties": false
                    },
                    "requested_source": { "type": "string", "enum": ["committed", "worktree"] },
                    "captured_source": { "type": ["string", "null"], "enum": ["committed", "worktree", null] },
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
                    "omitted_page": {
                        "type": "object",
                        "properties": {
                            "offset": { "type": "integer", "minimum": 0 },
                            "returned": { "type": "integer", "minimum": 0 },
                            "has_more": { "type": "boolean" },
                            "next_offset": {
                                "type": ["integer", "null"],
                                "minimum": 0
                            }
                        },
                        "required": ["offset", "returned", "has_more", "next_offset"],
                        "additionalProperties": false
                    },
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
                    "omitted_page",
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
                    "finding_guidance": { "type": "object" },
                    "next": { "type": "object" },
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
                        "description": "Terminal mapping (the same reading as CLI status): advisory plus improved static grip reports improved; complete changed stays open as limited; incomplete producer completeness stays limited except recorded stale or gap-mismatch lifecycle states retain stale or invalid; invalid producer status stays invalid; advisory regression preserves the trimmed, case-insensitive legacy static_movement.state fallback.",
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
                "schema_version": "ripr-mcp-gap-list-v4",
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
        if reparsed.get("schema_version") != Some(&json!("ripr-mcp-gap-list-v4")) {
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

    /// #9360: the text companion is selectable, `structuredContent` is the
    /// single full result in both modes, and the guard is mode-independent.
    #[test]
    fn summary_text_mode_keeps_structured_content_whole_and_shrinks_only_the_text()
    -> Result<(), String> {
        let rows = |count: usize| {
            json!({
                "schema_version": "ripr-mcp-gap-list-v4",
                "claim_boundary": "Static exposure evidence only.",
                "selected": count,
                "overflowed": false,
                "requested_snapshot_id": Value::Null,
                "filters": { "class": "weakly_exposed", "language": "rust" },
                "items": (0..count)
                    .map(|index| {
                        json!({
                            "canonical_id": format!("gap:rust:src/m{index}.rs:owner:predicate_boundary:p{index}"),
                            "note": "n".repeat(300),
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        };
        let budget = super::super::MAX_RESPONSE_BYTES;
        let full = tool_result_with(rows(40), budget, TextContentMode::Full)?;
        let summary = tool_result_with(rows(40), budget, TextContentMode::Summary)?;
        if full != tool_result(rows(40), budget)? {
            return Err("tool_result must stay the lossless Full mode".to_string());
        }
        if full["structuredContent"] != summary["structuredContent"]
            || summary["structuredContent"] != rows(40)
        {
            return Err("structuredContent must be the one full document in both modes".into());
        }
        if full["isError"] != summary["isError"] || summary["isError"] != json!(false) {
            return Err("summary mode must not change isError".into());
        }
        let full_text = full
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or("full text missing")?;
        if serde_json::from_str::<Value>(full_text).ok() != Some(rows(40)) {
            return Err("Full text must remain the JSON copy of the document".into());
        }
        let text = summary
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or("summary text missing")?;
        if summary["content"].as_array().map(Vec::len) != Some(1) {
            return Err("summary mode keeps exactly one text content item".into());
        }
        if serde_json::from_str::<Value>(text).is_ok() {
            return Err("summary text must not masquerade as the JSON document".into());
        }
        // The text-only pointer, the identity of the document, the counts
        // and the first identities (so a text-only agent can continue).
        for needle in [
            "ripr-mcp-gap-list-v4",
            "not a copy",
            "structuredContent",
            "RIPR_MCP_TEXT_CONTENT=full",
            "selected: 40",
            "overflowed: false",
            "requested_snapshot_id: null",
            "claim_boundary: Static exposure evidence only.",
            "filters: object with fields class, language",
            "items: 40 entries; first canonical_id values: gap:rust:src/m0.rs",
            "gap:rust:src/m4.rs",
        ] {
            if !text.contains(needle) {
                return Err(format!("summary text lost {needle:?}: {text}"));
            }
        }
        if text.contains("gap:rust:src/m5.rs") || text.contains(&"n".repeat(50)) {
            return Err(format!(
                "summary must bound ids and drop row bodies: {text}"
            ));
        }
        let (full_bytes, summary_bytes) = (full_text.len(), text.len());
        if summary_bytes * 10 > full_bytes {
            return Err(format!(
                "summary text should be an order of magnitude smaller: {summary_bytes} vs {full_bytes}"
            ));
        }
        // A long scalar is clipped with a marker that points back.
        let long = tool_result_with(
            json!({ "schema_version": "s", "note": "x".repeat(500) }),
            budget,
            TextContentMode::Summary,
        )?;
        let long_text = long
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or("long text missing")?;
        if !long_text.contains("(truncated; full text in structuredContent)")
            || long_text.contains(&"x".repeat(201))
        {
            return Err(format!(
                "long scalar must be clipped with a marker: {long_text}"
            ));
        }
        // The guard measures the complete envelope in both modes, so the
        // refusal boundary does not move with the mode.
        let big = rows(250);
        let big_bytes = serde_json::to_string(&big)
            .map_err(|error| error.to_string())?
            .len();
        if big_bytes >= budget || big_bytes * 2 <= budget {
            return Err(format!(
                "fixture drifted: the window document must fit alone but not doubled, got {big_bytes} bytes"
            ));
        }
        for mode in [
            TextContentMode::Full,
            TextContentMode::Summary,
            TextContentMode::Brief,
        ] {
            if tool_result_with(big.clone(), budget, mode).is_ok() {
                return Err(format!(
                    "{mode:?} must refuse a window document the same way"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn text_content_mode_only_opts_in_on_the_summary_spelling() -> Result<(), String> {
        use TextContentMode::{Brief, Full, Summary};
        let cases = [
            (None, Full),
            (Some("summary"), Summary),
            (Some(" Summary "), Summary),
            (Some("brief"), Brief),
            (Some(" BRIEF "), Brief),
            (Some("brie"), Full),
            (Some("full"), Full),
            // A typo never shrinks what a text-only host receives.
            (Some("summry"), Full),
            (Some(""), Full),
        ];
        for (value, expected) in cases {
            let got = TextContentMode::from_env_value(value);
            if got != expected {
                return Err(format!("{value:?} selected {got:?}, expected {expected:?}"));
            }
        }
        if TextContentMode::default() != Full {
            return Err("the default must stay the lossless Full mode".into());
        }
        Ok(())
    }

    fn brief_gap_list() -> Value {
        json!({
            "schema_version": "ripr-mcp-gap-list-v4",
            "eligible": 3,
            "omitted": 0,
            "items": [
                {
                    "canonical_id": "gap:rust:src/lib.rs:f:return_value",
                    "class": "weakly_exposed",
                    "file": "src/lib.rs",
                    "line": 259,
                    "repair_packet_ready": false,
                    "repair_ineligibility": "seam_already_gripped"
                },
                {
                    "canonical_id": "gap:rust:src/lib.rs:g:predicate",
                    "class": "exposed",
                    "file": "src/lib.rs",
                    "line": 12,
                    "repair_packet_ready": true,
                    "repair_ineligibility": Value::Null
                },
            ],
            "page": { "offset": 0, "limit": 2, "returned": 2, "has_more": true, "next_offset": 2 },
        })
    }

    /// #11621: `brief` renders one decision-bearing line per gap, is much
    /// shorter than the full JSON copy, and leaves `structuredContent` whole.
    #[test]
    fn brief_text_lists_one_useful_line_per_gap_and_keeps_structured_content() -> Result<(), String>
    {
        let document = brief_gap_list();
        let budget = super::super::MAX_RESPONSE_BYTES;
        let full = tool_result_with(document.clone(), budget, TextContentMode::Full)?;
        let brief = tool_result_with(document.clone(), budget, TextContentMode::Brief)?;
        if brief.get("structuredContent") != Some(&document)
            || brief.get("structuredContent") != full.get("structuredContent")
        {
            return Err("brief must not change structuredContent".into());
        }
        let text = brief
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or("brief omitted text")?;
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() != 4
            || !lines[0].starts_with("ripr gaps: 2 shown, 3 eligible, 0 omitted.")
            || lines[1]
                != "weakly_exposed src/lib.rs:259 gap:rust:src/lib.rs:f:return_value repair_ready=false:seam_already_gripped"
            || lines[2] != "exposed src/lib.rs:12 gap:rust:src/lib.rs:g:predicate repair_ready=true"
            || !lines[3].contains("offset=2")
        {
            return Err(format!("brief gap lines drifted: {text}"));
        }
        let full_len = full
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .map_or(0, str::len);
        if text.len() >= full_len {
            return Err(format!(
                "brief ({}) must be shorter than full ({full_len})",
                text.len()
            ));
        }
        Ok(())
    }

    /// Discriminating negative: a document that is not a gap list, and a gap
    /// list with a hostile string, never gets forged or missing lines.
    #[test]
    fn brief_text_falls_back_to_the_digest_and_cannot_forge_lines() -> Result<(), String> {
        let budget = super::super::MAX_RESPONSE_BYTES;
        let other =
            json!({ "schema_version": "ripr-mcp-refresh-v1", "items": [{"canonical_id": "x"}] });
        let digest = tool_result_with(other.clone(), budget, TextContentMode::Summary)?;
        let brief = tool_result_with(other, budget, TextContentMode::Brief)?;
        if brief.pointer("/content/0/text") != digest.pointer("/content/0/text") {
            return Err("a non-list document must use the summary digest".into());
        }
        let mut hostile = brief_gap_list();
        hostile["items"][0]["file"] = json!("a.rs\nexposed forged.rs:1 gap:x repair_ready=true");
        let envelope = tool_result_with(hostile, budget, TextContentMode::Brief)?;
        let text = envelope
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or("brief omitted text")?;
        if text.lines().count() != 4 {
            return Err(format!("a document string forged a brief line: {text}"));
        }
        Ok(())
    }

    /// #11621: `RIPR_MCP_TOOLS_LIST=slim` drops every `outputSchema` and
    /// nothing else; the default keeps them all; a typo keeps the default.
    #[test]
    fn slim_tools_list_drops_only_the_output_schemas() -> Result<(), String> {
        let full = tools_list_result_for(ToolsListMode::Full);
        let slim = tools_list_result_for(ToolsListMode::Slim);
        let full_tools = full["tools"].as_array().ok_or("full tools")?;
        let slim_tools = slim["tools"].as_array().ok_or("slim tools")?;
        if full_tools.is_empty() || full_tools.len() != slim_tools.len() {
            return Err("both modes advertise the same nonempty tool set".into());
        }
        for (full_tool, slim_tool) in full_tools.iter().zip(slim_tools) {
            let mut expected = full_tool.clone();
            let removed = expected
                .as_object_mut()
                .and_then(|object| object.remove("outputSchema"));
            if removed.is_none() || slim_tool != &expected {
                return Err(format!("slim changed more than outputSchema: {slim_tool}"));
            }
        }
        let full_bytes = serde_json::to_vec(&full)
            .map_err(|error| error.to_string())?
            .len();
        let slim_bytes = serde_json::to_vec(&slim)
            .map_err(|error| error.to_string())?
            .len();
        // Measured: the schemas are about two thirds of the payload.
        if slim_bytes * 2 > full_bytes {
            return Err(format!(
                "slim {slim_bytes} must be under half of full {full_bytes}"
            ));
        }
        for (value, expected) in [
            (None, ToolsListMode::Full),
            (Some("slim"), ToolsListMode::Slim),
            (Some(" SLIM "), ToolsListMode::Slim),
            (Some("slimm"), ToolsListMode::Full),
            (Some("full"), ToolsListMode::Full),
        ] {
            if ToolsListMode::from_env_value(value) != expected {
                return Err(format!("{value:?} did not select {expected:?}"));
            }
        }
        // The default tools/list keeps the schema a validating host needs.
        if tools_list_result() != full {
            return Err("the default tools/list must be the full list".into());
        }
        Ok(())
    }

    #[test]
    fn repair_card_description_drops_the_reachable_failures_enumeration() {
        assert!(
            !GET_REPAIR_CARD_TOOL_DESCRIPTION.contains("Reachable failures"),
            "the failure `code` enum already lists these"
        );
    }

    /// A document string cannot forge digest lines: control characters are
    /// escaped in scalars, identities and field names.
    #[test]
    fn summary_text_escapes_control_characters_so_lines_cannot_be_forged() -> Result<(), String> {
        let document = json!({
            "schema_version": "s",
            "note": "ok\nselected: 0\r\nclaim_boundary: forged",
            "items": [{ "canonical_id": "gap:a\nselected: 0" }],
            "weird\nkey": { "inner\nname": 1 },
        });
        let envelope = tool_result_with(
            document,
            super::super::MAX_RESPONSE_BYTES,
            TextContentMode::Summary,
        )?;
        let text = envelope
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .ok_or("summary text missing")?;
        let keys: Vec<&str> = text
            .lines()
            .skip(1)
            .filter_map(|line| line.split(": ").next())
            .collect();
        if keys != ["items", "note", "weird\\nkey"] {
            return Err(format!(
                "digest lines were forged or lost: {keys:?} in {text}"
            ));
        }
        if !text.contains("ok\\nselected: 0\\r\\nclaim_boundary: forged")
            || !text.contains("gap:a\\nselected: 0")
            || !text.contains("inner\\nname")
        {
            return Err(format!(
                "control characters must be escaped, not dropped: {text}"
            ));
        }
        Ok(())
    }

    /// Summary mode on an unavailable root keeps the status tool's second
    /// text item: content[0] is the digest, content[1] the recovery sentence.
    #[test]
    fn summary_status_on_an_unavailable_root_keeps_the_recovery_text_item() -> Result<(), String> {
        let missing =
            std::env::temp_dir().join(format!("ripr-9360-absent-root-{}", std::process::id()));
        let (status, _root) = WorkspaceStatus::resolve_with_root(Some(missing));
        let recovery = unavailable_recovery(&status)
            .ok_or("a missing root must be unavailable with a recovery sentence")?;
        let session = super::super::workspace::WorkspaceSession::default();
        let profile = super::super::workspace::SessionProfile::resolve(None);
        let envelope = status_tool_result(
            &status,
            &session,
            &profile,
            super::super::MAX_MESSAGE_BYTES,
            super::super::MAX_RESPONSE_BYTES,
            super::super::MAX_RESPONSE_BYTES,
            TextContentMode::Summary,
        )?;
        let content = envelope
            .get("content")
            .and_then(Value::as_array)
            .ok_or("content missing")?;
        if content.len() != 2 {
            return Err(format!("expected digest plus recovery, got {envelope}"));
        }
        if content[1]["text"].as_str() != Some(recovery.as_str()) {
            return Err(format!(
                "content[1] must be the recovery sentence: {envelope}"
            ));
        }
        let digest = content[0]["text"].as_str().ok_or("digest missing")?;
        if !digest.starts_with("ripr summary (") || digest == recovery {
            return Err(format!("content[0] must be the digest: {digest}"));
        }
        if envelope["structuredContent"].get("workspace").is_none() {
            return Err(format!("structuredContent must stay whole: {envelope}"));
        }
        Ok(())
    }

    fn check_description_byte_budgets(
        tools: &Value,
        resources: &Value,
        templates: &Value,
        instructions: &str,
    ) -> Result<usize, String> {
        // #9935: enumerate the published catalogs, rather than a parallel
        // list of description constants that can omit a published tool.
        // #10541: descriptions keep purpose + when-to-pick + failure codes;
        // document-shape detail lives in the outputSchema. Ceilings sit just
        // above the tightened surfaces (the exact total below is the real
        // pin); the complete ceiling adds only the inventory's 1,090 bytes.
        let mut budgets = std::collections::BTreeMap::from([
            ("tool ripr_workspace_status", 945),
            ("tool ripr_refresh", 1400),
            ("tool ripr_list_gaps", 1050),
            ("tool ripr_get_gap", 904),
            ("tool ripr_get_top_limitation", 640),
            ("tool ripr_prepare_repair", 1235),
            ("tool ripr_get_repair_attempt", 1315),
            ("tool ripr_get_receipt_status", 1495),
            ("tool ripr_get_repair_card", 1475),
            ("tool ripr_list_repair_attempts", 1090),
            ("resource ripr-workspace-status", 650),
            ("template ripr-snapshot-evidence", 475),
            ("template ripr-gap-evidence", 486),
            ("template ripr-gap-limitation", 290),
            ("template ripr-repair-attempt", 475),
            ("template ripr-receipt-status", 475),
            ("template ripr-repair-card", 475),
        ]);
        if instructions.is_empty() || instructions.len() > 1866 {
            return Err(format!(
                "instructions contain {} UTF-8 bytes; expected 1..=1866",
                instructions.len()
            ));
        }
        let mut total = instructions.len();
        let mut previous_surfaces = instructions.len();
        for (kind, catalog, key) in [
            ("tool", tools, "tools"),
            ("resource", resources, "resources"),
            ("template", templates, "resourceTemplates"),
        ] {
            let rows = catalog[key]
                .as_array()
                .ok_or_else(|| format!("{kind} catalog lost {key}"))?;
            for row in rows {
                let name = row["name"]
                    .as_str()
                    .ok_or_else(|| format!("{kind} catalog contains an unnamed surface"))?;
                let surface = format!("{kind} {name}");
                let budget = budgets.remove(surface.as_str()).ok_or_else(|| {
                    format!("{surface} has no description budget or appears more than once")
                })?;
                let description = row["description"]
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| format!("{surface} lost its nonempty description"))?;
                let bytes = description.len();
                if bytes > budget {
                    return Err(format!(
                        "{surface} description is {bytes} UTF-8 bytes, over its {budget}-byte budget"
                    ));
                }
                total += bytes;
                if surface != "tool ripr_list_repair_attempts" {
                    previous_surfaces += bytes;
                }
            }
        }
        if let Some(surface) = budgets.keys().next() {
            return Err(format!("{surface} is missing from its published catalog"));
        }
        if previous_surfaces > 15453 {
            return Err(format!(
                "previously guarded descriptions total {previous_surfaces} UTF-8 bytes, over 15453"
            ));
        }
        if total > 16543 {
            return Err(format!(
                "complete descriptions/instructions total {total} UTF-8 bytes, over 16543"
            ));
        }
        Ok(total)
    }

    #[test]
    fn description_length_budgets_keep_session_context_lean() -> Result<(), String> {
        let total = check_description_byte_budgets(
            &tools_list_result(),
            &resources_list_result(),
            &resource_templates_list_result(),
            INSTRUCTIONS,
        )?;
        // Independent literal accounting after #10541's tightening: purpose,
        // when-to-pick trigger, and failure codes per surface; the receipt
        // terminal mapping moved onto the outputSchema status field.
        // Intentional metadata changes need to reconcile this byte count as
        // well as the ceilings.
        assert_eq!(total, 16436);
        Ok(())
    }

    #[test]
    fn description_budget_rejects_inventory_growth_in_utf8_bytes() -> Result<(), String> {
        let mut tools = tools_list_result();
        let inventory = tools["tools"]
            .as_array_mut()
            .and_then(|rows| {
                rows.iter_mut()
                    .find(|row| row["name"] == "ripr_list_repair_attempts")
            })
            .ok_or("the real catalog lost the inventory tool")?;
        let original = inventory["description"]
            .as_str()
            .ok_or("inventory description")?;
        let grown = format!("{original}éé");
        assert_eq!(grown.len(), 1091);
        assert!(grown.chars().count() <= 1090);
        inventory["description"] = json!(grown);
        let error = match check_description_byte_budgets(
            &tools,
            &resources_list_result(),
            &resource_templates_list_result(),
            INSTRUCTIONS,
        ) {
            Err(error) => error,
            Ok(total) => return Err(format!("inventory growth escaped its budget: {total}")),
        };
        if !error.contains("tool ripr_list_repair_attempts") || !error.contains("1091 UTF-8 bytes")
        {
            return Err(format!(
                "inventory growth lost its surface/byte diagnosis: {error}"
            ));
        }
        Ok(())
    }

    #[test]
    fn description_budget_rejects_missing_inventory_surface_or_text() -> Result<(), String> {
        for remove_tool in [false, true] {
            let mut tools = tools_list_result();
            let rows = tools["tools"].as_array_mut().ok_or("tool catalog")?;
            if remove_tool {
                rows.retain(|row| row["name"] != "ripr_list_repair_attempts");
            } else {
                rows.iter_mut()
                    .find(|row| row["name"] == "ripr_list_repair_attempts")
                    .and_then(Value::as_object_mut)
                    .ok_or("inventory tool")?
                    .remove("description")
                    .ok_or("inventory description")?;
            }
            let error = match check_description_byte_budgets(
                &tools,
                &resources_list_result(),
                &resource_templates_list_result(),
                INSTRUCTIONS,
            ) {
                Err(error) => error,
                Ok(total) => {
                    return Err(format!("missing inventory surface/text escaped: {total}"));
                }
            };
            if !error.contains("tool ripr_list_repair_attempts") {
                return Err(format!(
                    "missing inventory lost its surface diagnosis: {error}"
                ));
            }
        }
        Ok(())
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
        if names != 10 {
            return Err(format!(
                "expected the ten-tool slice surface, got {names} tools"
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
        let descriptions: [(&str, &str, &[&str]); 8] = [
            (
                "/tools/1/description",
                REFRESH_TOOL_NAME,
                &[
                    "shared check authority",
                    "omitting `base` returns to the default branch",
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
                    "result_too_large",
                    "follow the failure's captured-root/base/source CLI recovery outside MCP",
                    "a bounded summary, not immutable snapshot replay",
                    "retrying ripr_get_gap cannot shrink the item",
                ],
            ),
            (
                "/tools/4/description",
                GET_TOP_LIMITATION_TOOL_NAME,
                &[
                    "earliest committed producer limitation",
                    "Producer static limits outrank readiness gates",
                    "no_limitation_established, never null",
                    "no analysis rerun",
                    "item_not_found",
                    "ripr://gap/{canonical_id}/limitation",
                ],
            ),
            (
                "/tools/5/description",
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
                "/tools/6/description",
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
                "/tools/7/description",
                GET_RECEIPT_STATUS_TOOL_NAME,
                &[
                    "awaiting_edit, after_pending, verification_pending, improved, closed, unchanged, regressed, limited, stale, and invalid",
                    "RIPR performs no verification and issues no receipt",
                    "attempt-bound",
                    "the same reading as CLI status",
                    "receipt presence alone never reports closed",
                    "Static movement and focused runtime test execution remain separate evidence axes",
                    "ripr://receipt/{receipt_id}",
                ],
            ),
            (
                "/tools/8/description",
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
        // The failure owns its source-bound command and detailed JSON caps;
        // discovery must not replace it with a fixed committed-input route.
        let gap_description = json_str(&tools, "/tools/3/description")?;
        if gap_description.contains("ripr check --committed --format json") {
            return Err(format!(
                "gap description advertises committed-only recovery: {gap_description}"
            ));
        }
        // #10541: the terminal mapping algorithm moved off the tool
        // description (which only names the vocabulary and the owner) onto
        // the outputSchema status field it annotates — same bytes, better
        // home, leaner routing surface.
        let status_description = json_str(
            &tools,
            "/tools/7/outputSchema/oneOf/0/properties/status/description",
        )?;
        for required in [
            "advisory plus improved static grip reports improved",
            "complete changed stays open as limited",
            "incomplete producer completeness stays limited",
            "recorded stale or gap-mismatch lifecycle states retain stale or invalid",
            "invalid producer status stays invalid",
            "advisory regression preserves the trimmed, case-insensitive legacy static_movement.state fallback",
        ] {
            if !status_description.contains(required) {
                return Err(format!(
                    "receipt outputSchema status field lost the terminal mapping wording {required:?}: {status_description}"
                ));
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
        if templates.len() != 6 {
            return Err(format!(
                "expected six resource templates, got {}",
                templates.len()
            ));
        }
        for (index, expected) in [
            (0, SNAPSHOT_RESOURCE_TEMPLATE),
            (1, GAP_RESOURCE_TEMPLATE),
            (2, LIMITATION_RESOURCE_TEMPLATE),
            (3, REPAIR_ATTEMPT_TEMPLATE),
            (4, RECEIPT_TEMPLATE),
            (5, REPAIR_CARD_TEMPLATE),
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
    fn tool_annotations_do_not_duplicate_the_top_level_title() -> Result<(), String> {
        // #9852: eight of nine descriptors repeated their top-level `title`
        // verbatim inside `annotations.title` (~27 wire bytes each, every
        // session). The annotation added no information; the dedup is
        // pinned so a copy does not creep back.
        let tools = tools_list_result();
        let tools = tools
            .pointer("/tools")
            .and_then(Value::as_array)
            .ok_or_else(|| "tools/list payload drifted".to_string())?;
        for tool in tools {
            let name = tool
                .pointer("/name")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool descriptor lost its name".to_string())?;
            if tool.pointer("/title").and_then(Value::as_str).is_none() {
                return Err(format!("{name} lost its top-level title"));
            }
            if tool.pointer("/annotations/title").is_some() {
                return Err(format!(
                    "{name} duplicates its title inside annotations; the top-level title is the one surface"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn tools_list_byte_budget_pins_the_session_context_sink() -> Result<(), String> {
        // #9852: `tools/list` is paid by every LLM consumer at session start
        // and re-paid on reconnect; one third of it already competes with
        // real evidence payloads inside the 128 KiB response bound. The
        // live-measured ceiling below (compact serialization of this exact
        // payload, after the `annotations.title` dedup) is pinned with
        // headroom for small additive contract growth; a failing assert
        // here means a descriptor change grew the per-session sink and
        // needs an explicit budget decision, not a silent ceiling bump.
        const TOOLS_LIST_BUDGET: usize = 49_800;
        let payload = tools_list_result();
        let bytes = serde_json::to_vec(&payload)
            .map_err(|error| format!("measure tools/list payload: {error}"))?
            .len();
        if bytes > TOOLS_LIST_BUDGET {
            return Err(format!(
                "tools/list is {bytes} bytes, over the {TOOLS_LIST_BUDGET}-byte session budget"
            ));
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
    fn pre_call_scope_routes_untracked_files_through_staging_or_explicit_diff() -> Result<(), String>
    {
        // #10541: the detailed scope routes live on INSTRUCTIONS once; the
        // refresh description names the committed-input boundary and points
        // at that block instead of duplicating the routes into every
        // session's tools/list payload.
        for required in [
            "By default: committed input only",
            "staged and unstaged tracked edits only",
            "unsaved buffers and untracked files remain outside",
            "worktree repair cards/readiness are withheld",
            "Untracked files need staging/intent-to-add",
            "git add -N <paths>",
            "before `--worktree`, or explicit `--diff`",
        ] {
            if !INSTRUCTIONS.contains(required) {
                return Err(format!(
                    "instructions lost the input-scope route {required:?}: {INSTRUCTIONS}"
                ));
            }
        }
        let tools = tools_list_result();
        let refresh = json_str(&tools, "/tools/1/description")?;
        if !refresh.contains("By default: committed input only")
            || !refresh.contains("source: \"worktree\"")
            || !refresh.contains("INSTRUCTIONS")
            || refresh.contains("Scope: committed input only")
            || INSTRUCTIONS.contains("Scope: committed input only")
        {
            return Err(format!(
                "refresh description must name the committed-input boundary and point at INSTRUCTIONS: {refresh}"
            ));
        }
        Ok(())
    }

    #[test]
    fn instructions_name_the_progressive_flow_and_the_cli_route() -> Result<(), String> {
        assert!(INSTRUCTIONS.starts_with("Flow: call `ripr_workspace_status`"));
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
            "ripr check --worktree --format json",
        ] {
            if !INSTRUCTIONS.contains(required) {
                return Err(format!("instructions lost {required:?}: {INSTRUCTIONS}"));
            }
        }
        // `ripr mcp --help` and the instructions must name the same command
        // for working-tree input (#10347).
        if !crate::mcp::MCP_HELP.contains("ripr check --worktree --format json") {
            return Err("MCP help and instructions name different working-tree commands".into());
        }
        Ok(())
    }
}
