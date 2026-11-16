//! #14138: findings `ripr check` withholds for a `#[mutants::skip]` owner
//! opt-out are withheld over MCP too. Real temporary Git repositories drive the
//! production refresh bridge ([`run_check`]); the withheld set is compared with
//! the shared check producer's `CheckOutput::withheld_finding_ids` over the
//! same input.

use super::workspace::{
    CODE_ITEM_NOT_FOUND, CODE_OWNER_OPT_OUT, GapListFilter, GapListWindow, RefreshSource,
    SessionProfile, Snapshot, WorkspaceSession, refresh_document, run_check,
};
use crate::testing::fixture_git::{fixture_git_ok, remove_fixture_tree};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ROOT_IDENTITY: &str = "root:sha256:owner-opt-out";
const REASON: &str = "owner_mutants_skip";
const SKIP: &str = "#[mutants::skip]";
/// Same byte length as [`SKIP`]: seam ids hash source offsets, so the control
/// must differ from the opt-out fixture only in the attribute itself.
const NO_SKIP: &str = "// not opted out";

/// `limit` carries the attribute slot; `sibling` never does. Each comparison
/// flips `>=` to `>` on its own line, so the two changes are independent.
fn lib(limit_attr: &str, limit_closed: bool, sibling_closed: bool) -> String {
    let limit_op = if limit_closed { ">" } else { ">=" };
    let sibling_op = if sibling_closed { ">" } else { ">=" };
    format!(
        "{limit_attr}
pub fn limit(x: u32) -> u32 {{
    if x {limit_op} 100 {{ 100 }} else {{ x }}
}}

pub fn sibling(x: u32) -> u32 {{
    if x {sibling_op} 7 {{ x + 1 }} else {{ x }}
}}
"
    )
}

struct Fixture {
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = remove_fixture_tree(&self.root);
    }
}

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, text).map_err(|error| error.to_string())
}

fn commit(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=ripr fixture",
            "-c",
            "user.email=fixture@ripr.invalid",
            "commit",
            "-qm",
            message,
        ],
    )
}

/// `main` holds the open comparisons; the checked-out `change` branch closes
/// `limit` and, when `change_sibling`, `sibling`. The attribute is present on
/// both commits, so only the comparison lines change. The tree is clean.
fn install(tag: &str, limit_attr: &str, change_sibling: bool) -> Result<Fixture, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-mcp-owner-opt-out-{tag}-{}-{stamp}",
        std::process::id()
    ));
    let _ = remove_fixture_tree(&root);
    let fixture = Fixture { root: root.clone() };
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"opt-out-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\nname = \"opt_out_fixture\"\npath = \"src/lib.rs\"\n\n[workspace]\n",
    )?;
    write(&root, ".gitignore", "/target/\n/.ripr/\n")?;
    write(&root, "src/lib.rs", &lib(limit_attr, false, false))?;
    fixture_git_ok(&root, &["-c", "init.defaultBranch=main", "init", "-q"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    commit(&root, "open comparisons")?;
    fixture_git_ok(&root, &["checkout", "-q", "-b", "change"])?;
    write(&root, "src/lib.rs", &lib(limit_attr, true, change_sibling))?;
    commit(&root, "close comparisons")?;
    Ok(fixture)
}

fn refresh(root: &Path) -> Result<Snapshot, String> {
    run_check(root, Some(ROOT_IDENTITY), None, RefreshSource::Committed)
        .map_err(|failure| format!("{}: {}", failure.code, failure.detail))
}

/// The shared check producer over the same input the refresh analyzed
/// (committed source, default base, the workspace config).
fn check_withheld_ids(root: &Path) -> Result<BTreeSet<String>, String> {
    let config = crate::config::load_for_root(root).map_err(|error| error.to_string())?;
    let mut input = crate::app::CheckInput {
        root: PathBuf::from(root),
        ..Default::default()
    };
    crate::config::apply_to_check_input(
        &mut input,
        &config,
        crate::config::CheckInputExplicit::default(),
    );
    let (output, _, _) = crate::app::check_workspace_with_config_typed(input, &config)
        .map_err(|error| format!("check: {error:?}"))?;
    if output.findings.is_empty() {
        return Err("the check producer must report findings for the fixture".to_string());
    }
    Ok(output
        .withheld_finding_ids()
        .into_iter()
        .map(str::to_string)
        .collect())
}

fn session_over(snapshot: Snapshot) -> WorkspaceSession {
    WorkspaceSession {
        last_good: Some(Arc::new(snapshot)),
        desired_source: RefreshSource::Committed,
        desired_source_defaulted: false,
        ..Default::default()
    }
}

fn list(session: &WorkspaceSession) -> Result<Value, String> {
    session
        .list_gaps(
            None,
            GapListWindow::default(),
            &GapListFilter::default(),
            crate::mcp::MAX_RESPONSE_BYTES,
        )
        .map_err(|failure| format!("{}: {}", failure.code, failure.detail))
}

fn listed_ids(document: &Value) -> Result<Vec<String>, String> {
    document["items"]
        .as_array()
        .ok_or_else(|| format!("gap list has no items array: {document}"))?
        .iter()
        .map(|item| {
            item["canonical_id"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("item without canonical_id: {item}"))
        })
        .collect()
}

/// The item on `line` of `src/lib.rs` (3 is `limit`, 7 is `sibling`).
fn item_on_line(snapshot: &Snapshot, line: usize) -> Result<String, String> {
    snapshot
        .items
        .iter()
        .find(|item| item.list_summary["line"] == serde_json::json!(line))
        .map(|item| item.canonical_id.clone())
        .ok_or_else(|| {
            format!(
                "no served item on line {line}: {:?}",
                snapshot
                    .items
                    .iter()
                    .map(|item| (&item.canonical_id, &item.list_summary["line"]))
                    .collect::<Vec<_>>()
            )
        })
}

fn expect_owner_opt_out<T: std::fmt::Debug>(
    result: Result<T, super::workspace::AttemptFailure>,
    surface: &str,
    owner: &str,
) -> Result<(), String> {
    match result {
        Err(failure) if failure.code == CODE_OWNER_OPT_OUT => {
            if failure.data["reason"] != serde_json::json!(REASON)
                || failure.data["owner"] != serde_json::json!(owner)
                || !failure.detail.contains("#[mutants::skip]")
            {
                return Err(format!(
                    "{surface}: refusal must name the owner opt-out: {failure:?}"
                ));
            }
            Ok(())
        }
        other => Err(format!(
            "{surface}: a withheld id must refuse with {CODE_OWNER_OPT_OUT}, got {other:?}"
        )),
    }
}

/// Acceptance: one changed `#[mutants::skip]` function and one changed plain
/// function. The gap list serves only the plain function and discloses the
/// withheld count and reason; the withheld set equals `ripr check`'s.
#[test]
fn gap_list_serves_only_the_plain_function_and_discloses_the_opt_out() -> Result<(), String> {
    let fixture = install("mixed", SKIP, true)?;
    let snapshot = refresh(&fixture.root)?;

    // The withheld set equals the check producer's for the same input.
    let mcp_withheld: BTreeSet<String> = snapshot
        .withheld
        .iter()
        .map(|item| item.finding_id.clone())
        .collect();
    let check_withheld = check_withheld_ids(&fixture.root)?;
    if mcp_withheld != check_withheld || check_withheld.len() != 1 {
        return Err(format!(
            "MCP withheld {mcp_withheld:?} must equal check's {check_withheld:?} (one finding)"
        ));
    }
    if snapshot.withheld[0].owner.as_deref() != Some("limit") {
        return Err(format!("owner must be limit: {:?}", snapshot.withheld));
    }

    let plain = item_on_line(&snapshot, 7)?;
    let withheld_canonical = snapshot.withheld[0].canonical_id.clone();
    let withheld_finding = snapshot.withheld[0].finding_id.clone();
    let session = session_over(snapshot);
    let document = list(&session)?;
    let ids = listed_ids(&document)?;
    if ids != vec![plain.clone()] {
        return Err(format!("only the plain function is a gap: {ids:?}"));
    }
    if document["owner_opt_outs"] != serde_json::json!({ "withheld": 1, "reason": REASON }) {
        return Err(format!("gap list must disclose the opt-out: {document:#}"));
    }
    if document["outcome"].get("clean").is_some() {
        return Err(format!(
            "a served gap remains; the outcome must not add clean: {document:#}"
        ));
    }

    // The snapshot index, refresh and status summaries disclose it too.
    let snapshot_id = document["snapshot_id"]
        .as_str()
        .ok_or("gap list lost its snapshot id")?
        .to_string();
    let index = session
        .snapshot_document(&snapshot_id)
        .map_err(|failure| failure.detail)?;
    let indexed = listed_ids(&index)?;
    if indexed != vec![plain.clone()] || index["owner_opt_outs"]["withheld"] != 1 {
        return Err(format!("snapshot index must match the list: {index:#}"));
    }
    let refreshed = refresh_document(&session);
    if refreshed["snapshot"]["owner_opt_outs"]["reason"] != serde_json::json!(REASON)
        || refreshed["snapshot"].get("clean").is_some()
    {
        return Err(format!("refresh summary must disclose: {refreshed:#}"));
    }
    let status = session.session_document(&SessionProfile::resolve(None));
    if status["last_completed_snapshot"]["owner_opt_outs"]["withheld"] != 1 {
        return Err(format!("status summary must disclose: {status:#}"));
    }

    // Single-item reads refuse the withheld id (canonical and finding form)
    // with a typed reason that names the owner opt-out, never a document.
    for id in [&withheld_canonical, &withheld_finding] {
        expect_owner_opt_out(session.get_gap(id, None), "ripr_get_gap", "limit")?;
        expect_owner_opt_out(
            session.get_limitation(id, None),
            "ripr_get_top_limitation",
            "limit",
        )?;
        expect_owner_opt_out(
            session.repair_card_document(id, None, Some(&fixture.root)),
            "ripr_get_repair_card",
            "limit",
        )?;
    }
    let mut session = session;
    expect_owner_opt_out(
        session.prepare_repair(&withheld_canonical, None, Some(ROOT_IDENTITY)),
        "ripr_prepare_repair",
        "limit",
    )?;
    if !session.repairs.is_empty() {
        return Err("a refused repair start must create no transaction".to_string());
    }
    // An id that names nothing still misses as an identity miss.
    match session.get_gap("gap:not-a-finding", None) {
        Err(failure) if failure.code == CODE_ITEM_NOT_FOUND => Ok(()),
        other => Err(format!("unknown ids keep item_not_found: {other:?}")),
    }
}

/// Negative: the plain function's gap document, limitation and repair card
/// are byte-identical (modulo snapshot identity and fixture root) to a control
/// where nothing opted out, and the control serves both functions with no
/// disclosure.
#[test]
fn plain_function_gap_and_card_are_unchanged_by_a_sibling_opt_out() -> Result<(), String> {
    let skipped = install("plain-skip", SKIP, true)?;
    let control = install("plain-control", NO_SKIP, true)?;
    let skipped_snapshot = refresh(&skipped.root)?;
    let control_snapshot = refresh(&control.root)?;
    if !control_snapshot.withheld.is_empty() || !check_withheld_ids(&control.root)?.is_empty() {
        return Err("the control must withhold nothing".to_string());
    }
    let skipped_plain = item_on_line(&skipped_snapshot, 7)?;
    let control_plain = item_on_line(&control_snapshot, 7)?;
    if skipped_plain != control_plain {
        return Err(format!(
            "the plain identity must not move: {skipped_plain} vs {control_plain}"
        ));
    }
    let control_limit = item_on_line(&control_snapshot, 3)?;
    let skipped_id = skipped_snapshot.snapshot_id.clone();
    let control_id = control_snapshot.snapshot_id.clone();
    // The identities differ (the served item lists differ too, so this does
    // not isolate the `owner_opt_out_withheld` identity payload).
    if skipped_id == control_id {
        return Err("an opted-out snapshot must not share the control identity".to_string());
    }
    let skipped_session = session_over(skipped_snapshot);
    let control_session = session_over(control_snapshot);

    let control_list = list(&control_session)?;
    let mut control_ids = listed_ids(&control_list)?;
    control_ids.sort();
    let mut expected = vec![control_plain.clone(), control_limit.clone()];
    expected.sort();
    if control_ids != expected || control_list.get("owner_opt_outs").is_some() {
        return Err(format!(
            "the control serves both functions with no disclosure: {control_list:#}"
        ));
    }
    // Without the attribute the `limit` finding is an ordinary item.
    control_session
        .get_gap(&control_limit, None)
        .map_err(|failure| format!("control limit gap: {failure:?}"))?;

    let normalize = |value: Value, snapshot_id: &str, root: &Path| -> Result<String, String> {
        let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
        Ok(text
            .replace(snapshot_id, "<snapshot>")
            .replace(&root.display().to_string(), "<root>"))
    };
    let gap = |session: &WorkspaceSession| {
        session
            .get_gap(&skipped_plain, None)
            .map_err(|failure| format!("{failure:?}"))
    };
    let limitation = |session: &WorkspaceSession| {
        session
            .get_limitation(&skipped_plain, None)
            .map_err(|failure| format!("{failure:?}"))
    };
    // This fixture has no admitted test target, so the plain card is refused
    // as `identity_unnameable` in both runs. Pin that outcome so the equality
    // below cannot pass on two `owner_opt_out` refusals of the plain item.
    match control_session.repair_card_document(&skipped_plain, None, Some(&control.root)) {
        Ok(_) => {}
        Err(failure) if failure.code == "identity_unnameable" => {}
        Err(failure) => {
            return Err(format!(
                "unexpected control plain card outcome: {failure:?}"
            ));
        }
    }
    let card = |session: &WorkspaceSession, root: &Path| match session.repair_card_document(
        &skipped_plain,
        None,
        Some(root),
    ) {
        Ok(document) => document,
        Err(failure) => failure.document("refused"),
    };
    let pairs = [
        ("gap", gap(&skipped_session)?, gap(&control_session)?),
        (
            "limitation",
            limitation(&skipped_session)?,
            limitation(&control_session)?,
        ),
        (
            "card",
            card(&skipped_session, &skipped.root),
            card(&control_session, &control.root),
        ),
    ];
    for (surface, skipped_doc, control_doc) in pairs {
        let left = normalize(skipped_doc, &skipped_id, &skipped.root)?;
        let right = normalize(control_doc, &control_id, &control.root)?;
        if left != right {
            return Err(format!(
                "the plain {surface} must be unchanged:\nskip:    {left}\ncontrol: {right}"
            ));
        }
    }
    Ok(())
}

/// Negative: when every finding is withheld the list is not an empty page
/// that reads as clean; it says all findings were withheld and why.
#[test]
fn all_withheld_snapshot_reports_that_state_not_a_clean_empty_list() -> Result<(), String> {
    let fixture = install("all", SKIP, false)?;
    let snapshot = refresh(&fixture.root)?;
    let check_withheld = check_withheld_ids(&fixture.root)?;
    let mcp_withheld: BTreeSet<String> = snapshot
        .withheld
        .iter()
        .map(|item| item.finding_id.clone())
        .collect();
    if mcp_withheld != check_withheld || mcp_withheld.is_empty() || !snapshot.items.is_empty() {
        return Err(format!(
            "every finding is withheld: mcp {mcp_withheld:?}, check {check_withheld:?}, items {}",
            snapshot.items.len()
        ));
    }
    let snapshot_id = snapshot.snapshot_id.clone();
    let session = session_over(snapshot);
    let index = session
        .snapshot_document(&snapshot_id)
        .map_err(|failure| failure.detail)?;
    if index["owner_opt_outs"]["all_withheld"] != serde_json::json!(true)
        || index["outcome"]["clean"] != serde_json::json!(false)
    {
        return Err(format!(
            "the snapshot resource must say all withheld: {index:#}"
        ));
    }
    let document = list(&session)?;
    if !listed_ids(&document)?.is_empty() {
        return Err(format!("no item may be served: {document:#}"));
    }
    let expected = serde_json::json!({
        "withheld": mcp_withheld.len(),
        "reason": REASON,
        "all_withheld": true,
    });
    if document["owner_opt_outs"] != expected
        || document["outcome"]["clean"] != serde_json::json!(false)
    {
        return Err(format!(
            "an all-withheld list must say so and not read clean: {document:#}"
        ));
    }
    let refreshed = refresh_document(&session);
    if refreshed["snapshot"]["owner_opt_outs"] != expected
        || refreshed["snapshot"]["clean"] != serde_json::json!(false)
    {
        return Err(format!("refresh must say all withheld: {refreshed:#}"));
    }
    let status = session.session_document(&SessionProfile::resolve(None));
    if status["last_completed_snapshot"]["owner_opt_outs"] != expected
        || status["last_completed_snapshot"]["clean"] != serde_json::json!(false)
    {
        return Err(format!("status must say all withheld: {status:#}"));
    }
    Ok(())
}

/// A deliberately small JSON Schema validator for the keywords the MCP
/// output schemas use. An unrecognized keyword is an error, never a silent
/// pass, so a schema that grows a new keyword fails this oracle loudly.
fn schema_violation(schema: &Value, value: &Value, path: &str) -> Option<String> {
    const KNOWN: &[&str] = &[
        "type",
        "const",
        "enum",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "oneOf",
        "minimum",
        "maximum",
        "minItems",
        "maxItems",
        "minLength",
        "maxLength",
        "description",
        "title",
    ];
    let Some(schema) = schema.as_object() else {
        return Some(format!("{path}: schema is not an object"));
    };
    if let Some(unknown) = schema.keys().find(|key| !KNOWN.contains(&key.as_str())) {
        return Some(format!("{path}: unsupported schema keyword {unknown:?}"));
    }
    if let Some(types) = schema.get("type") {
        let names: Vec<&str> = match types {
            Value::String(name) => vec![name.as_str()],
            Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
            _ => return Some(format!("{path}: malformed type")),
        };
        let matches = |name: &str| match name {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            "integer" => value.is_u64() || value.is_i64(),
            "number" => value.is_number(),
            _ => false,
        };
        if !names.iter().any(|name| matches(name)) {
            return Some(format!("{path}: {value} is not of type {types}"));
        }
    }
    if let Some(expected) = schema.get("const")
        && expected != value
    {
        return Some(format!("{path}: {value} != const {expected}"));
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return Some(format!("{path}: {value} not in enum {allowed:?}"));
    }
    if let (Some(minimum), Some(number)) = (
        schema.get("minimum").and_then(Value::as_f64),
        value.as_f64(),
    ) && number < minimum
    {
        return Some(format!("{path}: {number} < minimum {minimum}"));
    }
    if let (Some(maximum), Some(number)) = (
        schema.get("maximum").and_then(Value::as_f64),
        value.as_f64(),
    ) && number > maximum
    {
        return Some(format!("{path}: {number} > maximum {maximum}"));
    }
    let bounded = |key: &str, size: Option<usize>, under: bool| -> Option<String> {
        let bound = schema.get(key).and_then(Value::as_u64)?;
        let size = size? as u64;
        let violates = if under { size < bound } else { size > bound };
        violates.then(|| format!("{path}: size {size} violates {key} {bound}"))
    };
    let items_len = value.as_array().map(Vec::len);
    let chars_len = value.as_str().map(|text| text.chars().count());
    if let Some(violation) = [
        bounded("minItems", items_len, true),
        bounded("maxItems", items_len, false),
        bounded("minLength", chars_len, true),
        bounded("maxLength", chars_len, false),
    ]
    .into_iter()
    .flatten()
    .next()
    {
        return Some(violation);
    }
    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        let failures: Vec<String> = branches
            .iter()
            .filter_map(|branch| schema_violation(branch, value, path))
            .collect();
        if branches.len() - failures.len() != 1 {
            return Some(format!(
                "{path}: {} of {} oneOf branches matched: {failures:?}",
                branches.len() - failures.len(),
                branches.len()
            ));
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Some(format!("{path}: missing required {key:?}"));
                }
            }
        }
        for (key, child) in object {
            match properties.and_then(|properties| properties.get(key)) {
                Some(child_schema) => {
                    if let Some(violation) =
                        schema_violation(child_schema, child, &format!("{path}/{key}"))
                    {
                        return Some(violation);
                    }
                }
                None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    return Some(format!("{path}: undeclared property {key:?}"));
                }
                None => {}
            }
        }
    }
    if let (Some(items), Some(entries)) = (schema.get("items"), value.as_array()) {
        for (index, entry) in entries.iter().enumerate() {
            if let Some(violation) = schema_violation(items, entry, &format!("{path}/{index}")) {
                return Some(violation);
            }
        }
    }
    None
}

fn advertised_output_schema(tool: &str) -> Result<Value, String> {
    let catalog = crate::mcp::protocol::tools_list_result();
    catalog["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|descriptor| descriptor["name"] == tool))
        .and_then(|descriptor| descriptor.get("outputSchema"))
        .cloned()
        .ok_or_else(|| format!("the full tools/list advertises no outputSchema for {tool}"))
}

/// The structured copy a real tool envelope carries, validated against the
/// complete advertised output schema of that tool.
fn validate_tool_result(tool: &str, envelope: &Value) -> Result<(), String> {
    let schema = advertised_output_schema(tool)?;
    let structured = envelope
        .get("structuredContent")
        .ok_or_else(|| format!("{tool}: envelope lost structuredContent"))?;
    match schema_violation(&schema, structured, tool) {
        None => Ok(()),
        Some(violation) => Err(format!(
            "{tool} result violates its outputSchema: {violation}"
        )),
    }
}

/// Review follow-up (#14138): the advertised MCP output schemas are closed, so
/// every document that now carries `owner_opt_outs` (gap list, refresh
/// `snapshot`, status `last_completed_snapshot`) and every `owner_opt_out`
/// refusal must validate against the full advertised schema of its tool, for
/// a mixed and an all-withheld snapshot.
#[test]
fn owner_opt_out_results_validate_against_the_advertised_output_schemas() -> Result<(), String> {
    use crate::mcp::protocol::{
        GET_GAP_TOOL_NAME, GET_TOP_LIMITATION_TOOL_NAME, LIST_GAPS_TOOL_NAME,
        PREPARE_REPAIR_TOOL_NAME, REFRESH_TOOL_NAME, STATUS_TOOL_NAME, status_document,
        tool_failure, tool_result,
    };
    let budget = crate::mcp::MAX_RESPONSE_BYTES;
    for (tag, change_sibling) in [("schema-mixed", true), ("schema-all", false)] {
        let fixture = install(tag, SKIP, change_sibling)?;
        let snapshot = refresh(&fixture.root)?;
        let withheld = snapshot
            .withheld
            .first()
            .map(|item| item.canonical_id.clone())
            .ok_or_else(|| format!("{tag}: the fixture must withhold a finding"))?;
        let mut session = session_over(snapshot);

        let gap_list = list(&session)?;
        if gap_list.get("owner_opt_outs").is_none() {
            return Err(format!("{tag}: the subject must carry owner_opt_outs"));
        }
        validate_tool_result(LIST_GAPS_TOOL_NAME, &tool_result(gap_list.clone(), budget)?)?;

        let refreshed = refresh_document(&session);
        if refreshed.pointer("/snapshot/owner_opt_outs").is_none() {
            return Err(format!("{tag}: refresh subject lost owner_opt_outs"));
        }
        validate_tool_result(REFRESH_TOOL_NAME, &tool_result(refreshed, budget)?)?;

        let (status, _) =
            crate::workspace_status::WorkspaceStatus::resolve_with_root(Some(fixture.root.clone()));
        let status_doc = status_document(
            &status,
            &session,
            &SessionProfile::resolve(Some(fixture.root.as_path())),
            crate::mcp::MAX_MESSAGE_BYTES,
            budget,
        );
        if status_doc
            .pointer("/session/last_completed_snapshot/owner_opt_outs")
            .is_none()
        {
            return Err(format!("{tag}: status subject lost owner_opt_outs"));
        }
        validate_tool_result(STATUS_TOOL_NAME, &tool_result(status_doc, budget)?)?;

        // The typed refusal validates against each refusing tool's schema.
        let refusals = [
            (
                GET_GAP_TOOL_NAME,
                super::gaps::GAP_SCHEMA_VERSION,
                session.get_gap(&withheld, None).err(),
            ),
            (
                GET_TOP_LIMITATION_TOOL_NAME,
                super::gaps::LIMITATION_SCHEMA_VERSION,
                session.get_limitation(&withheld, None).err(),
            ),
            (
                "ripr_get_repair_card",
                super::repair_card::REPAIR_CARD_SCHEMA_VERSION,
                session
                    .repair_card_document(&withheld, None, Some(&fixture.root))
                    .err(),
            ),
            (
                PREPARE_REPAIR_TOOL_NAME,
                super::repair::REPAIR_PACKET_SCHEMA_VERSION,
                session
                    .prepare_repair(&withheld, None, Some(ROOT_IDENTITY))
                    .err(),
            ),
        ];
        for (tool, schema_version, failure) in refusals {
            let failure = failure
                .filter(|failure| failure.code == CODE_OWNER_OPT_OUT)
                .ok_or_else(|| format!("{tag}: {tool} must refuse with owner_opt_out"))?;
            validate_tool_result(tool, &tool_failure(&failure, schema_version)?)?;
        }

        // The oracle discriminates: the same gap list fails once its
        // disclosure is malformed, or once the schema no longer declares it.
        let schema = advertised_output_schema(LIST_GAPS_TOOL_NAME)?;
        let mut wrong_reason = gap_list.clone();
        wrong_reason["owner_opt_outs"]["reason"] = serde_json::json!("other");
        let mut extra_key = gap_list.clone();
        extra_key["owner_opt_outs"]["owner"] = serde_json::json!("limit");
        let mut undeclared = schema.clone();
        if let Some(properties) = undeclared
            .pointer_mut("/oneOf/0/properties")
            .and_then(Value::as_object_mut)
        {
            properties.remove("owner_opt_outs");
        }
        if schema_violation(&schema, &wrong_reason, "list").is_none()
            || schema_violation(&schema, &extra_key, "list").is_none()
            || schema_violation(&undeclared, &gap_list, "list").is_none()
        {
            return Err(format!(
                "{tag}: the schema oracle must reject a malformed or undeclared owner_opt_outs"
            ));
        }
        session.repairs.clear();
    }
    Ok(())
}

/// Review follow-up (#14138): with `RIPR_MCP_TEXT_CONTENT=brief` a text-only
/// host reads the brief, not the structured copy, so the brief must name the
/// withheld count and reason; an all-withheld list must not read as clean.
#[test]
fn brief_gap_list_text_names_the_owner_opt_out() -> Result<(), String> {
    use crate::mcp::protocol::{TextContentMode, tool_result_with};
    let brief = |root: &Path| -> Result<String, String> {
        let document = list(&session_over(refresh(root)?))?;
        let envelope = tool_result_with(
            document,
            crate::mcp::MAX_RESPONSE_BYTES,
            TextContentMode::Brief,
        )?;
        envelope
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("brief envelope lost its text: {envelope}"))
    };
    let mixed = install("brief-mixed", SKIP, true)?;
    let all = install("brief-all", SKIP, false)?;
    let control = install("brief-control", NO_SKIP, true)?;
    let line = "owner_opt_outs: 1 withheld (owner_mutants_skip: the changed owner carries #[mutants::skip]); not repair targets";
    let all_suffix = "every finding is withheld, so this is not a clean result";
    let mixed_text = brief(&mixed.root)?;
    if !mixed_text.contains(line) || mixed_text.contains(all_suffix) {
        return Err(format!("mixed brief must name one withheld: {mixed_text}"));
    }
    let all_text = brief(&all.root)?;
    if !all_text.contains(&format!("{line}; {all_suffix}")) {
        return Err(format!("all-withheld brief must say so: {all_text}"));
    }
    let control_text = brief(&control.root)?;
    if control_text.contains("owner_opt_outs") {
        return Err(format!("nothing withheld adds no line: {control_text}"));
    }
    Ok(())
}
