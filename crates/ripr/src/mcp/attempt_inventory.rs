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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Owns one exclusively created workspace root, removed even on unwind.
    struct Root(PathBuf);

    impl Root {
        fn new(label: &str) -> Result<Self, String> {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("fixture clock: {error}"))?
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "ripr-mcp-inventory-{label}-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir(&path)
                .map_err(|error| format!("create {}: {error}", path.display()))?;
            Ok(Self(path))
        }

        /// A manifest-bearing directory whose manifest the shared authority refuses.
        fn invalid_attempt(&self, directory: &str) -> Result<(), String> {
            let path = self.0.join("target/ripr/repair-attempts").join(directory);
            std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
            std::fs::write(path.join("attempt.json"), "{not-json")
                .map_err(|error| error.to_string())
        }

        fn with_invalid(label: &str, directories: &[&str]) -> Result<Self, String> {
            let root = Self::new(label)?;
            for directory in directories {
                root.invalid_attempt(directory)?;
            }
            Ok(root)
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0) {
                eprintln!("remove inventory fixture {}: {error}", self.0.display());
            }
        }
    }

    const ROOT_IDENTITY: Option<&str> = Some("root:test");
    const ROOMY: usize = usize::MAX / 4;

    fn list(
        root: &Root,
        requested: Option<&str>,
        offset: usize,
        limit: Option<usize>,
        budget: usize,
    ) -> Result<Value, AttemptFailure> {
        list_document(
            Some(&root.0),
            ROOT_IDENTITY,
            requested,
            offset,
            limit,
            budget,
        )
    }

    fn page(
        root: &Root,
        requested: Option<&str>,
        offset: usize,
        limit: Option<usize>,
    ) -> Result<Value, String> {
        list(root, requested, offset, limit, ROOMY).map_err(|failure| failure.detail)
    }

    fn directories(document: &Value) -> Vec<&str> {
        document["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["directory"].as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn refusal(result: Result<Value, AttemptFailure>) -> Result<AttemptFailure, String> {
        match result {
            Err(failure) => Ok(failure),
            Ok(document) => Err(format!("expected a typed refusal, got {document}")),
        }
    }

    #[test]
    fn missing_workspace_root_is_a_typed_workspace_unavailable_failure() -> Result<(), String> {
        let failure = refusal(list_document(None, None, None, 0, None, ROOMY))?;
        assert_eq!(failure.code, "workspace_unavailable");
        assert!(failure.recovery.contains("--root"), "{}", failure.recovery);
        Ok(())
    }

    #[test]
    fn missing_store_is_an_honest_empty_inventory_without_creating_it() -> Result<(), String> {
        let root = Root::new("empty")?;
        let document = page(&root, None, 0, None)?;
        assert_eq!(document["total"], json!(0));
        assert_eq!(document["omitted"], json!(0));
        assert_eq!(document["page"]["has_more"], json!(false));
        assert_eq!(document["page"]["next_offset"], Value::Null);
        assert!(document["continuation"].get("next_page").is_none());
        assert!(
            !root.0.join("target").exists(),
            "a read must not create the store"
        );
        Ok(())
    }

    #[test]
    fn invalid_rows_are_listed_by_directory_without_trusting_manifest_fields() -> Result<(), String>
    {
        let root = Root::with_invalid("invalid", &["zeta", "alpha"])?;
        let document = page(&root, None, 0, None)?;
        assert_eq!(document["total"], json!(2));
        assert_eq!(document["valid"], json!(0));
        assert_eq!(document["invalid"], json!(2));
        // Ordered by directory name, never by creation time.
        assert_eq!(directories(&document), ["alpha", "zeta"]);
        let first = &document["items"][0];
        assert_eq!(first["validation"], json!("invalid"));
        assert_eq!(first["failure"]["code"], json!(CODE_ATTEMPT_INVALID));
        assert!(first.get("attempt_id").is_none());
        Ok(())
    }

    #[test]
    fn pagination_binds_next_page_to_the_inventory_and_ends_without_one() -> Result<(), String> {
        let root = Root::with_invalid("paging", &["a0", "a1", "a2", "a3", "a4"])?;
        let first = page(&root, None, 0, Some(2))?;
        assert_eq!(directories(&first), ["a0", "a1"]);
        assert_eq!(first["omitted"], json!(3));
        assert_eq!(first["page"]["has_more"], json!(true));
        assert_eq!(first["page"]["next_offset"], json!(2));
        let next = &first["continuation"]["next_page"];
        assert_eq!(next["tool"], json!(TOOL_NAME));
        assert_eq!(next["arguments"]["offset"], json!(2));
        assert_eq!(next["arguments"]["limit"], json!(2));
        assert_eq!(next["arguments"]["inventory_id"], first["inventory_id"]);

        let id = first["inventory_id"].as_str();
        let last = page(&root, id, 4, Some(2))?;
        assert_eq!(directories(&last), ["a4"]);
        assert_eq!(last["page"]["returned"], json!(1));
        assert_eq!(last["page"]["has_more"], json!(false));
        assert_eq!(last["page"]["next_offset"], Value::Null);
        assert!(last["continuation"].get("next_page").is_none());

        // An offset past the end returns no rows and never wraps around.
        let beyond = page(&root, id, 99, None)?;
        assert_eq!(beyond["page"]["returned"], json!(0));
        assert_eq!(beyond["page"]["has_more"], json!(false));
        assert_eq!(beyond["omitted"], json!(5));
        Ok(())
    }

    #[test]
    fn unlimited_request_omits_limit_from_the_continuation_and_caps_at_256() -> Result<(), String> {
        let names: Vec<String> = (0..MAX_PAGE_ITEMS + 2)
            .map(|n| format!("a{n:04}"))
            .collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let root = Root::with_invalid("cap", &refs)?;
        let capped = page(&root, None, 0, None)?;
        assert_eq!(capped["page"]["returned"], json!(MAX_PAGE_ITEMS));
        assert_eq!(capped["page"]["next_offset"], json!(MAX_PAGE_ITEMS));
        let arguments = &capped["continuation"]["next_page"]["arguments"];
        assert!(arguments.get("limit").is_none(), "{arguments}");
        // A larger requested limit is still bounded by the page cap.
        let big = page(&root, None, 0, Some(10_000))?;
        assert_eq!(big["page"]["returned"], json!(MAX_PAGE_ITEMS));
        assert_eq!(big["omitted"], json!(2));
        Ok(())
    }

    #[test]
    fn changed_observation_fails_closed_with_the_current_inventory_identity() -> Result<(), String>
    {
        let root = Root::with_invalid("stale", &["a0", "a1"])?;
        let bound = page(&root, None, 0, Some(1))?;
        let id = bound["inventory_id"].as_str().ok_or("inventory_id")?;
        assert!(id.starts_with("repair-inventory:sha256:"), "{id}");
        page(&root, Some(id), 1, Some(1))?;

        root.invalid_attempt("a2")?;
        let failure = refusal(list(&root, Some(id), 1, Some(1), ROOMY))?;
        assert_eq!(failure.code, CODE_STALE_INVENTORY);
        let current = page(&root, None, 0, Some(1))?;
        assert_eq!(
            failure.data["current_inventory_id"],
            current["inventory_id"]
        );
        assert_ne!(failure.data["current_inventory_id"], json!(id));
        // A bogus identity is refused even when the store is unchanged.
        let bogus = refusal(list(
            &root,
            Some("repair-inventory:sha256:0"),
            0,
            None,
            ROOMY,
        ))?;
        assert_eq!(bogus.code, CODE_STALE_INVENTORY);
        Ok(())
    }

    #[test]
    fn envelope_budget_shrinks_the_page_and_keeps_the_continuation_honest() -> Result<(), String> {
        let root = Root::with_invalid("budget", &["a0", "a1", "a2", "a3", "a4", "a5"])?;
        let all = page(&root, None, 0, Some(6))?;
        let three = page(&root, None, 0, Some(3))?;
        let budget = protocol::structured_envelope_bytes(&three)?;
        assert!(budget < protocol::structured_envelope_bytes(&all)?);

        let shrunk = list(&root, None, 0, Some(6), budget).map_err(|failure| failure.detail)?;
        let returned = shrunk["page"]["returned"].as_u64().ok_or("returned")?;
        assert!((1..6).contains(&returned), "{returned}");
        assert!(protocol::structured_envelope_bytes(&shrunk)? <= budget);
        assert_eq!(shrunk["page"]["has_more"], json!(true));
        assert_eq!(shrunk["page"]["next_offset"], json!(returned));
        assert_eq!(shrunk["omitted"], json!(6 - returned));
        assert_eq!(
            shrunk["items"].as_array().map(Vec::len),
            Some(returned as usize)
        );
        assert_eq!(
            shrunk["continuation"]["next_page"]["arguments"]["offset"],
            json!(returned)
        );
        Ok(())
    }

    #[test]
    fn a_budget_that_fits_no_row_is_a_typed_result_too_large_failure() -> Result<(), String> {
        let root = Root::with_invalid("toolarge", &["a0", "a1"])?;
        let failure = refusal(list(&root, None, 0, None, 1))?;
        assert_eq!(failure.code, "result_too_large");
        assert_eq!(failure.recovery, RECOVERY_TOO_LARGE);
        Ok(())
    }
}
