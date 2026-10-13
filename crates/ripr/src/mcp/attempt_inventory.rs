//! Bounded read-only discovery over the canonical durable repair-attempt store.
use super::protocol;
use super::repair::{CODE_ATTEMPT_INVALID, durable_receipt_document};
use super::workspace::AttemptFailure;
use crate::app::repair_attempt::{
    RepairAttemptInventoryEntry, inventory_repair_attempts_from, repair_attempt_state_label,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) const TOOL_NAME: &str = "ripr_list_repair_attempts";
pub(super) const SCHEMA_VERSION: &str = "ripr-mcp-repair-attempt-list-v1";
pub(super) const CODE_STALE_INVENTORY: &str = "stale_inventory";
pub(super) const RECOVERY_TOO_LARGE: &str = "inspect durable attempts with `ripr agent status`; repeating or narrowing this same row cannot make it fit";
const MAX_PAGE_ITEMS: usize = 256;

pub(super) fn tool_descriptor() -> Value {
    json!({
        "name": TOOL_NAME,
        "title": "RIPR durable repair attempt inventory",
        "description": "Discover durable CLI repair attempts in this workspace's default store before or after refresh, without creating a store, editing source, preparing an attempt, or executing verification. In-memory MCP transactions are excluded. Rows are ordered by identity/directory name, never by time: valid rows carry attempt_id, state, seam_id, repository_head, shared read-time currentness and receipt_status, plus detail links; invalid rows retain directory and attempt_invalid without trusting manifest fields. A missing default store is empty; an unavailable root or store read failure is a typed failure. Optional offset >= 0 and limit >= 1 page all rows (including invalid ones), capped at 256 rows and the full structured-envelope byte limit. Follow continuation.next_page with inventory_id; a changed observation fails closed with stale_inventory and the current identity, so restart at offset 0. inventory_id binds root, observed HEAD and all summary rows; this is a live read, not an atomic filesystem snapshot. Output is bounded; canonical validation still reads the whole store. Use ripr_get_repair_attempt and ripr_get_receipt_status for one valid discovered ID. Nothing returned authorizes an edit or command.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "inventory_id": {"type": "string", "description": "Optional prior inventory identity; mismatch fails with stale_inventory."},
                "offset": {"type": "integer", "minimum": 0},
                "limit": {"type": "integer", "minimum": 1}
            },
            "additionalProperties": false
        },
        "outputSchema": {
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "schema_version": {"type": "string", "const": SCHEMA_VERSION},
                        "inventory_id": {"type": "string"},
                        "root_identity": {"type": ["string", "null"]},
                        "observed_repository_head": {"type": ["string", "null"]},
                        "total": {"type": "integer", "minimum": 0},
                        "valid": {"type": "integer", "minimum": 0},
                        "invalid": {"type": "integer", "minimum": 0},
                        "omitted": {"type": "integer", "minimum": 0, "description": "Rows outside this page, including earlier pages."},
                        "items": {"type": "array", "maxItems": MAX_PAGE_ITEMS, "items": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "properties": {
                                        "validation": {"const": "valid"},
                                        "attempt_id": {"type": "string"},
                                        "state": {"type": "string"},
                                        "seam_id": {"type": "string"},
                                        "repository_head": {"type": "string"},
                                        "receipt_status": {"type": "string"},
                                        "currentness": {"type": "object"},
                                        "links": {"type": "object"}
                                    },
                                    "required": ["validation", "attempt_id", "state", "seam_id", "repository_head", "receipt_status", "currentness", "links"],
                                    "additionalProperties": false
                                },
                                {
                                    "type": "object",
                                    "properties": {
                                        "validation": {"const": "invalid"},
                                        "directory": {"type": "string"},
                                        "failure": {
                                            "type": "object",
                                            "properties": {"code": {"const": CODE_ATTEMPT_INVALID}, "detail": {"type": "string"}},
                                            "required": ["code", "detail"],
                                            "additionalProperties": false
                                        }
                                    },
                                    "required": ["validation", "directory", "failure"],
                                    "additionalProperties": false
                                }
                            ]
                        }},
                        "page": {
                            "type": "object",
                            "properties": {
                                "offset": {"type": "integer", "minimum": 0},
                                "limit": {"type": ["integer", "null"], "minimum": 1},
                                "returned": {"type": "integer", "minimum": 0},
                                "has_more": {"type": "boolean"},
                                "next_offset": {"type": ["integer", "null"], "minimum": 0}
                            },
                            "required": ["offset", "limit", "returned", "has_more", "next_offset"],
                            "additionalProperties": false
                        },
                        "continuation": {"type": "object"},
                        "claim_boundary": {"type": "string"},
                        "limitations": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["schema_version", "inventory_id", "root_identity", "observed_repository_head", "total", "valid", "invalid", "omitted", "items", "page", "continuation", "claim_boundary", "limitations"],
                    "additionalProperties": false
                },
                protocol::typed_failure_document_schema(SCHEMA_VERSION)
            ]
        },
        "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
    })
}

pub(super) fn list_document(
    root: Option<&Path>,
    root_identity: Option<&str>,
    requested: Option<&str>,
    offset: usize,
    limit: Option<usize>,
    max_envelope_bytes: usize,
) -> Result<Value, AttemptFailure> {
    let root = root.ok_or_else(|| {
        AttemptFailure::new(
            "workspace_unavailable",
            "the workspace root is unavailable; the durable attempt store cannot be read",
            "restart the server with `ripr mcp --stdio --root <repository>` and retry",
        )
    })?;
    let inventory = inventory_repair_attempts_from(root, None).map_err(|_error| AttemptFailure::new(
        CODE_ATTEMPT_INVALID,
        "the shared repair-attempt authority could not read this workspace's durable store",
        "inspect the workspace store diagnostic with `ripr agent status`, then retry ripr_list_repair_attempts",
    ))?;
    let observed_head = crate::agent::artifact::current_git_head(root).ok();
    let mut rows = Vec::with_capacity(inventory.len());
    let mut valid = 0usize;
    for entry in inventory {
        rows.push(match entry {
            RepairAttemptInventoryEntry::Valid(manifest) => {
                valid += 1;
                // This retained projection owns receipt strength and live HEAD applicability.
                // Never infer closure, issuance or currentness from artifact presence.
                let receipt = durable_receipt_document(&manifest, root, root_identity);
                let id = manifest.repair_attempt_id.as_str();
                json!({
                    "validation": "valid",
                    "attempt_id": id,
                    "state": repair_attempt_state_label(&manifest.state),
                    "seam_id": manifest.seam_id,
                    "repository_head": manifest.repository_head,
                    "receipt_status": receipt["status"],
                    "currentness": receipt["currentness"],
                    "links": {
                        "repair_attempt": format!("ripr://repair-attempt/{id}"),
                        "receipt": format!("ripr://receipt/{id}")
                    }
                })
            }
            RepairAttemptInventoryEntry::Invalid { directory, .. } => json!({
                "validation": "invalid",
                "directory": directory,
                "failure": {
                    "code": CODE_ATTEMPT_INVALID,
                    "detail": "the shared repair-attempt authority refused this manifest; no attempt fields were trusted"
                }
            }),
        });
    }
    let bytes = serde_json::to_vec(&(SCHEMA_VERSION, root_identity, &observed_head, &rows))
        .map_err(serialization_failure)?;
    let inventory_id = format!("repair-inventory:sha256:{:x}", Sha256::digest(bytes));
    if requested.is_some_and(|requested| requested != inventory_id) {
        return Err(AttemptFailure::new(
            CODE_STALE_INVENTORY,
            "the requested inventory no longer matches this workspace's observed durable attempt summaries",
            "restart ripr_list_repair_attempts at offset 0 without inventory_id, then follow its bound continuation",
        ).with_data(json!({"current_inventory_id": inventory_id})));
    }
    let total = rows.len();
    let available = total.saturating_sub(offset);
    let count = available
        .min(limit.unwrap_or(MAX_PAGE_ITEMS))
        .min(MAX_PAGE_ITEMS);
    let document = |count: usize| {
        let returned = count;
        let has_more = offset.saturating_add(returned) < total;
        let next_offset = has_more.then(|| offset + returned);
        let mut continuation = json!({
            "attempt_tool": "ripr_get_repair_attempt",
            "receipt_tool": "ripr_get_receipt_status"
        });
        if let Some(next_offset) = next_offset {
            let mut arguments = json!({"inventory_id": inventory_id, "offset": next_offset});
            if let Some(limit) = limit {
                arguments["limit"] = json!(limit);
            }
            continuation["next_page"] = json!({"tool": TOOL_NAME, "arguments": arguments});
        }
        json!({
            "schema_version": SCHEMA_VERSION,
            "inventory_id": inventory_id,
            "root_identity": root_identity,
            "observed_repository_head": observed_head,
            "total": total, "valid": valid, "invalid": total - valid,
            "omitted": total - returned,
            "items": rows.iter().skip(offset).take(count).collect::<Vec<_>>(),
            "page": {"offset": offset, "limit": limit, "returned": returned, "has_more": has_more, "next_offset": next_offset},
            "continuation": continuation,
            "claim_boundary": "Read-only durable CLI attempt discovery. Validation and receipt currentness use shared authorities; this listing prepares nothing and executes nothing.",
            "limitations": [
                "MCP in-memory transactions are excluded; their IDs are returned by ripr_prepare_repair",
                "Live observations are not an atomic filesystem snapshot; restart pagination if the inventory changes",
                "Output is count/byte bounded; canonical inventory validation still reads the whole store",
                "Only manifest-bearing directories are inventoried; unrepresentable UTF-8 names refuse the store",
                "Invalid rows retain directory identity only; the CLI store diagnostic owns full refusal detail"
            ]
        })
    };
    let fits = |candidate: &Value| -> Result<bool, AttemptFailure> {
        let bytes = serde_json::to_vec(candidate)
            .map_err(serialization_failure)?
            .len();
        Ok(bytes <= super::MAX_TOOL_DOCUMENT_BYTES
            && protocol::structured_envelope_bytes(candidate).map_err(serialization_failure)?
                <= max_envelope_bytes)
    };
    let whole = document(count);
    if fits(&whole)? {
        return Ok(whole);
    }
    let mut low = 1usize;
    let mut high = count.saturating_sub(1);
    let mut fitting = 0usize;
    while low <= high {
        let middle = low + (high - low) / 2;
        if fits(&document(middle))? {
            fitting = middle;
            low = middle + 1;
        } else {
            high = middle.saturating_sub(1);
        }
    }
    if fitting == 0 {
        return Err(AttemptFailure::new(
            "result_too_large",
            "the inventory cannot fit even one row and its disclosures in a structured MCP response",
            RECOVERY_TOO_LARGE,
        ));
    }
    Ok(document(fitting))
}

fn serialization_failure(error: impl std::fmt::Display) -> AttemptFailure {
    AttemptFailure::new(
        "analysis_failed",
        format!("serialize durable inventory: {error}"),
        "retry ripr_list_repair_attempts",
    )
}
