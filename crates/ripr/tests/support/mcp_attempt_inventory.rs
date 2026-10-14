//! Real stdio consumers for #7463; the durable producer is the public CLI.
use super::*;

const INVENTORY_TOOL: &str = "ripr_list_repair_attempts";

fn with_session(
    root: &Path,
    run: impl FnOnce(&mut McpSession, &Value) -> Result<(), String>,
) -> Result<(), String> {
    let mut session = McpSession::spawn(root)?;
    let result = (|| {
        session.initialize()?;
        let tools = session.list_tools("inventory-discovery")?;
        run(&mut session, &tools)
    })();
    // Reap the peer even when a behavioral assertion is the RED witness.
    let closeout = session.finish(EXPECTED_MAX_RESPONSE_BYTES);
    result?;
    let audit = closeout?;
    if audit.frames == 0 {
        return Err("inventory session observed no protocol frames".into());
    }
    Ok(())
}

fn inventory(session: &mut McpSession, tools: &Value, args: Value) -> Result<Value, String> {
    let name = tool_named(tools, INVENTORY_TOOL)?;
    let reply = session.call("inventory", &name, args)?;
    let document = tool_success(&reply, "inventory")?.clone();
    let returned = entries(&document)?.len() as u64;
    let total = as_u64(&document, "/total", "inventory")?;
    if as_u64(&document, "/page/returned", "inventory")? != returned
        || as_u64(&document, "/omitted", "inventory")? != total - returned
        || as_u64(&document, "/valid", "inventory")? + as_u64(&document, "/invalid", "inventory")?
            != total
    {
        return Err(format!("inventory page denominators drifted: {document}"));
    }
    if as_bool(&document, "/page/has_more", "inventory")?
        && as_u64(&document, "/page/next_offset", "inventory")?
            != as_u64(&document, "/page/offset", "inventory")? + returned
    {
        return Err(format!(
            "inventory continuation skipped returned rows: {document}"
        ));
    }
    Ok(document)
}

fn entries(document: &Value) -> Result<&Vec<Value>, String> {
    at(document, "/items", "inventory")?
        .as_array()
        .ok_or_else(|| format!("inventory lost its items array: {document}"))
}

fn invalid_manifest(root: &Path, directory: &str) -> Result<PathBuf, String> {
    let path = root.join("target/ripr/repair-attempts").join(directory);
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    let manifest = path.join("attempt.json");
    std::fs::write(&manifest, "{not-json").map_err(|error| error.to_string())?;
    Ok(manifest)
}

#[cfg(unix)]
#[test]
fn inventory_refuses_lossy_directory_identity_and_recovers() -> Result<(), String> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let fixture = install_b4_fixture("inventory-raw-name")?;
    let store = fixture.root.join("target/ripr/repair-attempts");
    invalid_manifest(&fixture.root, "ascii-invalid")?;
    let first = store.join(std::ffi::OsString::from_vec(vec![0xff]));
    let second = store.join(std::ffi::OsString::from_vec(vec![0xfe]));
    with_session(&fixture.root, |session, tools| {
        let clean = inventory(session, tools, json!({"limit": 1}))?;
        let bound = clean["inventory_id"].clone();
        std::fs::create_dir(&first).map_err(|error| error.to_string())?;
        // A reservation without a manifest is not a published attempt.
        let reserved = inventory(session, tools, json!({"inventory_id": bound}))?;
        if reserved["total"] != json!(1) || reserved["inventory_id"] != bound {
            return Err("unpublished reservation changed the manifest inventory".into());
        }
        std::fs::write(first.join("attempt.json"), "{not-json")
            .map_err(|error| error.to_string())?;
        let name = tool_named(tools, INVENTORY_TOOL)?;
        for directory in [&first, &second] {
            if directory == &second {
                std::fs::rename(&first, &second).map_err(|error| error.to_string())?;
            }
            // Raw names, rather than the lossy snapshot helper, own this oracle.
            let names = std::fs::read_dir(&store)
                .map_err(|error| error.to_string())?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            let expected: &[u8] = if directory == &first {
                &[0xff]
            } else {
                &[0xfe]
            };
            if !names.iter().any(|name| name.as_bytes() == expected) {
                return Err("the distinct raw directory stimulus is absent".into());
            }
            let before =
                std::fs::read(directory.join("attempt.json")).map_err(|error| error.to_string())?;
            for args in [
                json!({"limit": 1}),
                json!({"inventory_id": bound, "offset": 1, "limit": 1}),
            ] {
                let reply = session.call("unrepresentable-directory", &name, args)?;
                if tool_failure(&reply, "unrepresentable directory")? != "attempt_invalid"
                    || reply.pointer("/result/structuredContent/items").is_some()
                    || reply.pointer("/result/structuredContent/total").is_some()
                {
                    return Err(format!(
                        "raw directory identity must refuse the entire inventory: {reply}"
                    ));
                }
            }
            if std::fs::read(directory.join("attempt.json")).map_err(|error| error.to_string())?
                != before
            {
                return Err("inventory refusal mutated the invalid manifest".into());
            }
        }
        std::fs::rename(&second, store.join("restored-invalid"))
            .map_err(|error| error.to_string())?;
        let recovered = inventory(session, tools, json!({"limit": 2}))?;
        if recovered["total"] != json!(2)
            || recovered["invalid"] != json!(2)
            || entries(&recovered)?[1]["directory"] != json!("restored-invalid")
        {
            return Err("restored UTF-8 directory did not recover discovery".into());
        }
        Ok(())
    })
}

#[test]
fn inventory_discovery_empty_store_and_argument_refusals() -> Result<(), String> {
    let fixture = install_b4_fixture("inventory-empty")?;
    let store = fixture.root.join("target/ripr/repair-attempts");
    if store.exists() {
        return Err("empty fixture already has a store".into());
    }
    let before = snapshot_tree(&fixture.root)?;
    with_session(&fixture.root, |session, tools| {
        let name = tool_named(tools, INVENTORY_TOOL)?;
        let document = inventory(session, tools, json!({}))?;
        if as_u64(&document, "/total", "empty inventory")? != 0
            || !entries(&document)?.is_empty()
            || as_bool(&document, "/page/has_more", "empty inventory")?
        {
            return Err(format!(
                "missing store must be an honest empty inventory: {document}"
            ));
        }
        for args in [
            json!({"offset": null}),
            json!({"offset": -1}),
            json!({"offset": true}),
            json!({"offset": 1.5}),
            json!({"limit": 0}),
            json!({"limit": null}),
            json!({"inventory_id": null}),
            json!({"snapshot_id": "invented"}),
        ] {
            let reply = session.call("bad-inventory-input", &name, args.clone())?;
            if reply.pointer("/error/code").and_then(Value::as_i64) != Some(-32602) {
                return Err(format!(
                    "invalid inventory args {args} must be Invalid Params: {reply}"
                ));
            }
        }
        let beyond = inventory(session, tools, json!({"offset": 99}))?;
        if as_u64(&beyond, "/page/offset", "beyond inventory")? != 99
            || !entries(&beyond)?.is_empty()
            || as_bool(&beyond, "/page/has_more", "beyond inventory")?
        {
            return Err(format!(
                "beyond-end page must echo offset and terminate: {beyond}"
            ));
        }
        if document["inventory_id"] != beyond["inventory_id"] {
            return Err("unchanged inventory identity depends on page arguments".into());
        }
        let get = tool_named(tools, "ripr_get_repair_attempt")?;
        let reply = session.call(
            "unknown-inventory-attempt",
            &get,
            json!({"attempt_id": UNKNOWN_ATTEMPT_ID}),
        )?;
        if tool_failure(&reply, "unknown attempt")? != "attempt_not_found"
            || !as_str(
                &reply,
                "/result/structuredContent/failure/recovery",
                "unknown attempt",
            )?
            .contains(INVENTORY_TOOL)
        {
            return Err(format!(
                "unknown attempt must name the discovered MCP inventory recovery: {reply}"
            ));
        }
        Ok(())
    })?;
    require_tree_equal(
        &before,
        &snapshot_tree(&fixture.root)?,
        "inventory missing-store and invalid inputs",
    )?;
    if store.exists() {
        return Err("read-only inventory created an empty store".into());
    }
    with_session(&fixture.root.join("missing-root"), |session, tools| {
        let name = tool_named(tools, INVENTORY_TOOL)?;
        let reply = session.call("unavailable-inventory", &name, json!({}))?;
        if tool_failure(&reply, "unavailable inventory")? != "workspace_unavailable" {
            return Err(format!(
                "unavailable root cannot invent an empty inventory: {reply}"
            ));
        }
        Ok(())
    })
}

#[test]
fn inventory_real_durable_attempt_edit_refresh_and_corruption_recovery() -> Result<(), String> {
    let fixture = install_b4_fixture("inventory-real")?;
    std::fs::write(fixture.root.join(".gitignore"), "/target/\n")
        .map_err(|error| error.to_string())?;
    commit_fixture(&fixture.root, "ignore owned outputs")?;
    let mut seam = String::new();
    with_session(&fixture.root, |session, tools| {
        // Discover and exercise the real analyzer before selecting the CLI producer's seam.
        let refresh = tool_named(tools, "ripr_refresh")?;
        let reply = session.call("inventory-first-refresh", &refresh, json!({}))?;
        let (snapshot, _, total) = require_completed_refresh(tool_success(&reply, "refresh")?)?;
        if total == 0 {
            return Err("real inventory fixture produced no changed subject".into());
        }
        let list = tool_named(tools, "ripr_list_gaps")?;
        let reply = session.call("inventory-gaps", &list, json!({"snapshot_id": snapshot}))?;
        let canonical = canonical_for_file(tool_success(&reply, "gaps")?, "src/lib.rs", "gaps")?;
        let card = tool_named(tools, "ripr_get_repair_card")?;
        let reply = session.call("inventory-card", &card, json!({"canonical_id": canonical}))?;
        seam = nonempty_str(
            tool_success(&reply, "card")?,
            "/card/subject/seam_id",
            "card",
        )?
        .to_string();
        Ok(())
    })?;
    let produced = cli_agent_document(
        &fixture.root,
        "repair",
        &["--seam-id", &seam, "--phase", "before", "--json"],
    )?;
    let attempt = nonempty_str(
        &produced,
        "/repair_attempt/attempt_id",
        "CLI before producer",
    )?
    .to_string();
    let head = fixture_git_output(&fixture.root, &["rev-parse", "HEAD"])?;
    let before = snapshot_tree(&fixture.root)?;
    let mut old_inventory = String::new();
    with_session(&fixture.root, |session, tools| {
        let document = inventory(session, tools, json!({"limit": 1}))?;
        if as_u64(&document, "/total", "real inventory")? != 1
            || as_u64(&document, "/valid", "real inventory")? != 1
        {
            return Err(format!(
                "CLI-produced attempt must be discoverable before refresh: {document}"
            ));
        }
        let row = entries(&document)?
            .first()
            .ok_or_else(|| "real inventory is empty".to_string())?;
        if nonempty_str(row, "/attempt_id", "valid row")? != attempt
            || as_str(row, "/state", "valid row")? != "awaiting_edit"
            || as_str(row, "/repository_head", "valid row")? != head.trim()
            || as_str(row, "/seam_id", "valid row")? != seam
            || as_str(row, "/receipt_status", "valid row")? != "awaiting_edit"
            || row
                .pointer("/currentness/head_current")
                .and_then(Value::as_bool)
                != Some(true)
            || as_str(row, "/currentness/state", "valid row")? != "current"
        {
            return Err(format!(
                "inventory invented producer fields or receipt strength: {row}"
            ));
        }
        old_inventory = nonempty_str(&document, "/inventory_id", "real inventory")?.to_string();
        for (tool, field) in [
            ("ripr_get_repair_attempt", "attempt_id"),
            ("ripr_get_receipt_status", "receipt_id"),
        ] {
            let name = tool_named(tools, tool)?;
            let reply =
                session.call("consume-discovered-attempt", &name, json!({field: attempt}))?;
            let detail = tool_success(&reply, tool)?;
            let pointer = if field == "attempt_id" {
                "/state"
            } else {
                "/status"
            };
            if field == "receipt_id" && row["currentness"] != detail["currentness"] {
                return Err(format!(
                    "inventory currentness differs from discovered receipt: {row} vs {detail}"
                ));
            }
            if as_str(detail, pointer, tool)? != "awaiting_edit" {
                return Err(format!(
                    "discovered ID cannot be consumed honestly: {detail}"
                ));
            }
        }
        Ok(())
    })?;
    require_tree_equal(
        &before,
        &snapshot_tree(&fixture.root)?,
        "durable inventory and detail reads",
    )?;

    let corrupt = "repair-attempt-000000000000000000000000";
    invalid_manifest(&fixture.root, corrupt)?;
    with_session(&fixture.root, |session, tools| {
        let name = tool_named(tools, INVENTORY_TOOL)?;
        let reply = session.call(
            "stale-inventory",
            &name,
            json!({"inventory_id": old_inventory, "offset": 1}),
        )?;
        if tool_failure(&reply, "stale inventory")? != "stale_inventory" {
            return Err(format!(
                "store changes must invalidate a continuation: {reply}"
            ));
        }
        let first = inventory(session, tools, json!({"limit": 1}))?;
        let row = entries(&first)?
            .first()
            .ok_or_else(|| "invalid row omitted".to_string())?;
        if as_str(row, "/validation", "invalid row")? != "invalid"
            || as_str(row, "/directory", "invalid row")? != corrupt
            || row.get("attempt_id").is_some()
        {
            return Err(format!(
                "invalid manifest cannot borrow valid identity fields: {row}"
            ));
        }
        let next = at(
            &first,
            "/continuation/next_page/arguments",
            "page continuation",
        )?
        .clone();
        let second = inventory(session, tools, next)?;
        if entries(&second)?.len() != 1
            || as_str(&entries(&second)?[0], "/attempt_id", "second page")? != attempt
            || as_bool(&second, "/page/has_more", "second page")?
        {
            return Err(format!(
                "mixed valid/invalid paging lost an identity: {second}"
            ));
        }
        let repeated = inventory(session, tools, json!({}))?;
        if repeated["inventory_id"] != first["inventory_id"]
            || second["inventory_id"] != first["inventory_id"]
        {
            return Err("unchanged observation has unstable or page-dependent identity".into());
        }
        let get = tool_named(tools, "ripr_get_repair_attempt")?;
        let reply = session.call("corrupt-attempt", &get, json!({"attempt_id": corrupt}))?;
        if tool_failure(&reply, "corrupt attempt")? != "attempt_invalid" {
            return Err(format!(
                "corrupt attempt must retain the direct-read refusal: {reply}"
            ));
        }
        // Same denominators, different identity: a totals-only inventory hash must fail.
        let renamed = "repair-attempt-111111111111111111111111";
        let store = fixture.root.join("target/ripr/repair-attempts");
        std::fs::rename(store.join(corrupt), store.join(renamed))
            .map_err(|error| error.to_string())?;
        let reply = session.call(
            "same-count-inventory-change",
            &name,
            json!({"inventory_id": first["inventory_id"], "offset": 1}),
        )?;
        if tool_failure(&reply, "same-count inventory")? != "stale_inventory" {
            return Err(format!(
                "identity changes with unchanged totals must reject the old page: {reply}"
            ));
        }
        // Canonical digest validation, rather than JSON parsing, owns admission.
        let manifest: Value = serde_json::from_slice(
            &std::fs::read(store.join(&attempt).join("attempt.json"))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let relative = nonempty_str(&manifest, "/artifacts/0/path", "retained artifact")?;
        let artifact = fixture.root.join(relative);
        let original = std::fs::read(&artifact).map_err(|error| error.to_string())?;
        std::fs::write(&artifact, b"tampered retained evidence")
            .map_err(|error| error.to_string())?;
        let damaged = inventory(session, tools, json!({}))?;
        if as_u64(&damaged, "/valid", "tampered inventory")? != 0
            || as_u64(&damaged, "/invalid", "tampered inventory")? != 2
        {
            return Err(format!(
                "parse-only admission ignored a retained artifact mismatch: {damaged}"
            ));
        }
        let reply = session.call("tampered-attempt", &get, json!({"attempt_id": attempt}))?;
        if tool_failure(&reply, "tampered attempt")? != "attempt_invalid" {
            return Err(format!("tampered detail was admitted: {reply}"));
        }
        std::fs::write(&artifact, original).map_err(|error| error.to_string())?;
        let restored = inventory(session, tools, json!({}))?;
        if as_u64(&restored, "/valid", "restored inventory")? != 1 {
            return Err(format!("restored producer cannot recover: {restored}"));
        }
        // An external committed test edit changes the analysis snapshot; the old ID cannot rebind.
        let refresh = tool_named(tools, "ripr_refresh")?;
        let reply = session.call("before-edit-refresh", &refresh, json!({}))?;
        let (old_snapshot, _, _) = require_completed_refresh(tool_success(&reply, "before edit")?)?;
        std::fs::write(fixture.root.join("tests/pricing.rs"), "use mcp_journey_fixture::discounted_total;\n#[test]\nfn equality() { assert_eq!(discounted_total(100, 100), 100); }\n").map_err(|error| error.to_string())?;
        commit_fixture(&fixture.root, "external equality assertion edit")?;
        let reply = session.call("after-edit-refresh", &refresh, json!({}))?;
        let (new_snapshot, _, _) = require_completed_refresh(tool_success(&reply, "after edit")?)?;
        if new_snapshot == old_snapshot {
            return Err("edit/refresh reused stale snapshot identity".into());
        }
        let gaps = tool_named(tools, "ripr_list_gaps")?;
        let reply = session.call("stale-gaps", &gaps, json!({"snapshot_id": old_snapshot}))?;
        if tool_failure(&reply, "old snapshot")? != "stale_snapshot" {
            return Err(format!("old snapshot rebound: {reply}"));
        }
        let reply = session.call("recover-gaps", &gaps, json!({"snapshot_id": new_snapshot}))?;
        tool_success(&reply, "recovered gaps")?;
        // Shared awaiting-attempt lineage admits an ordinary descendant, then refuses a diverged HEAD.
        let descendant = inventory(session, tools, json!({}))?;
        let row = entries(&descendant)?
            .iter()
            .find(|row| row.get("attempt_id").and_then(Value::as_str) == Some(&attempt))
            .ok_or_else(|| "descendant lost attempt".to_string())?;
        if as_str(row, "/receipt_status", "descendant")? != "awaiting_edit" {
            return Err(format!("descendant lost shared admission: {row}"));
        }
        fixture_git_ok(&fixture.root, &["checkout", "-q", "main"])?;
        let stale = inventory(session, tools, json!({}))?;
        let row = entries(&stale)?
            .iter()
            .find(|row| row.get("attempt_id").and_then(Value::as_str) == Some(&attempt))
            .ok_or_else(|| "historical inventory lost attempt".to_string())?;
        if as_str(row, "/receipt_status", "historical attempt")? != "stale"
            || row
                .pointer("/currentness/head_current")
                .and_then(Value::as_bool)
                != Some(false)
            || as_str(row, "/currentness/state", "historical attempt")? != "historical"
        {
            return Err(format!("historical attempt was promoted: {row}"));
        }
        Ok(())
    })
}

#[test]
fn inventory_byte_pages_preserve_every_invalid_identity() -> Result<(), String> {
    let fixture = install_b4_fixture("inventory-bytes")?;
    let mut expected = Vec::new();
    // Reverse creation order and escaped names discriminate identity order and full envelope fitting.
    for index in (0..300).rev() {
        let character = if cfg!(windows) { "x" } else { "\"" };
        let directory = format!("{index:04}-{}", character.repeat(240));
        invalid_manifest(&fixture.root, &directory)?;
        expected.push(directory);
    }
    expected.sort();
    let before = snapshot_tree(&fixture.root)?;
    with_session(&fixture.root, |session, tools| {
        let mut args = json!({"limit": 256});
        let mut observed = Vec::new();
        let mut pages = 0;
        loop {
            let document = inventory(session, tools, args)?;
            if as_u64(&document, "/total", "byte inventory")? != 300
                || as_u64(&document, "/invalid", "byte inventory")? != 300
                || as_u64(&document, "/valid", "byte inventory")? != 0
            {
                return Err(format!("invalid denominator was weakened: {document}"));
            }
            let rows = entries(&document)?;
            if rows.is_empty() || (pages == 0 && rows.len() >= 256) {
                return Err(format!(
                    "byte-limited page made no progress or ignored its bound: {} rows",
                    rows.len()
                ));
            }
            for row in rows {
                observed.push(as_str(row, "/directory", "byte row")?.to_string());
                if row.get("attempt_id").is_some() {
                    return Err("invalid row invented an attempt ID".into());
                }
            }
            pages += 1;
            if !as_bool(&document, "/page/has_more", "byte page")? {
                break;
            }
            if pages > 300 {
                return Err("inventory continuation did not terminate".into());
            }
            args = at(
                &document,
                "/continuation/next_page/arguments",
                "byte continuation",
            )?
            .clone();
        }
        if observed != expected || pages < 2 {
            return Err(format!(
                "identity order/coverage drifted across {pages} pages"
            ));
        }
        Ok(())
    })?;
    require_tree_equal(
        &before,
        &snapshot_tree(&fixture.root)?,
        "byte inventory reads",
    )
}

/// #7463: the full wire wrapper, SDK resultType and actual escaped request ID
/// must fit alongside the structured document, not just the inner tool result.
#[cfg(unix)]
#[test]
fn inventory_near_cap_keeps_structured_pages_for_small_and_escaped_ids() -> Result<(), String> {
    let fixture = install_b4_fixture("inventory-near-cap")?;
    for index in 0..300 {
        let quotes = if index == 70 { 29 } else { 240 };
        let directory = format!("{index:04}-{}", "\"".repeat(quotes));
        invalid_manifest(&fixture.root, &directory)?;
    }
    let before = snapshot_tree(&fixture.root)?;
    with_session(&fixture.root, |session, tools| {
        let name = tool_named(tools, INVENTORY_TOOL)?;
        let mut identities = Vec::new();
        for id in ["inventory".to_string(), "\"".repeat(6000)] {
            let reply = session.call(&id, &name, json!({"limit": 256}))?;
            let document = tool_success(&reply, "near-cap inventory")?;
            if entries(document)?.is_empty()
                || !as_bool(document, "/page/has_more", "near-cap inventory")?
            {
                return Err("near-cap fixture never reached a progressing page".into());
            }
            let next = at(
                document,
                "/continuation/next_page/arguments",
                "near-cap continuation",
            )?
            .clone();
            let continued = inventory(session, tools, next)?;
            if entries(&continued)?.is_empty() {
                return Err("near-cap continuation lost the remaining rows".into());
            }
            identities.push(document["inventory_id"].clone());
        }
        if identities[0] != identities[1] {
            return Err("request ID changed inventory identity".into());
        }
        Ok(())
    })?;
    modern_near_cap_reads(&fixture.root)?;
    require_tree_equal(
        &before,
        &snapshot_tree(&fixture.root)?,
        "near-cap wire reads",
    )
}

#[cfg(unix)]
fn modern_near_cap_reads(root: &Path) -> Result<(), String> {
    let mut session = McpSession::spawn(root)?;
    let result = (|| {
        let meta = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
            "io.modelcontextprotocol/clientInfo": {"name": "inventory-wire-consumer", "version": "1"}
        });
        session.send_value(&json!({"jsonrpc": "2.0", "id": "modern-tools", "method": "tools/list", "params": {"_meta": meta}}))?;
        let tools = session.await_reply("modern-tools", REPLY_TIMEOUT)?;
        let name = tool_named(&tools, INVENTORY_TOOL)?;
        for id in ["inventory".to_string(), "\"".repeat(6000)] {
            session.send_value(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"_meta": meta, "name": name, "arguments": {"limit": 256}}}))?;
            let reply = session.await_reply(&id, REPLY_TIMEOUT)?;
            let document = tool_success(&reply, "modern near-cap inventory")?;
            if reply.pointer("/result/resultType").and_then(Value::as_str) != Some("complete")
                || entries(document)?.is_empty()
                || !as_bool(document, "/page/has_more", "modern near-cap page")?
                || document
                    .pointer("/continuation/next_page/arguments")
                    .is_none()
            {
                return Err(format!(
                    "modern lifecycle lost complete structured inventory: {reply}"
                ));
            }
        }
        Ok(())
    })();
    let closeout = session.finish(EXPECTED_MAX_RESPONSE_BYTES);
    result?;
    closeout?;
    Ok(())
}
