//! Bounded resident-session observations, not a universal performance budget.
//! Linux's sampled direct-child VmRSS is neither heap nor process-tree memory.
#[path = "resident_session/transport.rs"]
mod transport;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use transport::{Session, Wire};

const SCHEMA: &str = "ripr-resident-session-v1";
const BASE_SOURCE: &str = "pub fn discounted_total(amount: i32, limit: i32) -> i32 {\n    if amount >= limit {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const TEST_SOURCE: &str = "use resident_fixture::discounted_total;\n#[test]\nfn below() { assert_eq!(discounted_total(1, 5), 1); }\n";
const SETTLE: Duration = Duration::from_millis(100);

pub fn main_entry() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args == ["--retention-control-child"] {
        return transport::retention_child();
    }
    if args == ["--cleanup-control-primary"] {
        return transport::cleanup_primary();
    }
    if args == ["--cleanup-control-grandchild"] {
        transport::cleanup_grandchild();
        return Ok(());
    }
    if args == ["--self-test"] {
        let executable = env::current_exe().map_err(|e| e.to_string())?;
        println!(
            "{}",
            json!({"retained_state_negative_control":retention_control(&executable, Instant::now() + Duration::from_secs(10))?,"owned_cleanup_control":transport::cleanup_control(&executable, Instant::now() + Duration::from_secs(10))?})
        );
        return Ok(());
    }
    if args.len() != 2 {
        return Err("usage: cargo run -p xtask --no-default-features --features resident-session --example resident_session -- <exact-ripr-binary> <new-evidence-directory>\nUse --self-test for the synthetic measured-child retention and owned-grandchild cleanup controls. Linux only. The fixed fixture is scratch; no existing repository is edited.".into());
    }
    if !cfg!(target_os = "linux") {
        return Err("VmRSS session instrument requires Linux; no measurements made".into());
    }
    let binary = fs::canonicalize(&args[0]).map_err(|e| format!("binary: {e}"))?;
    let before = file_digest(&binary)?;
    let evidence = PathBuf::from(&args[1]);
    fs::create_dir(&evidence).map_err(|e| format!("evidence directory must be new: {e}"))?;
    let evidence = fs::canonicalize(&evidence).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut report = json!({
        "schema_version": SCHEMA, "status": "incomplete", "binary": binary,
        "binary_sha256": before, "host_os": env::consts::OS, "host_arch": env::consts::ARCH,
        "workload": "resident-rust-boundary-v1", "size_tier": "synthetic_small",
        "instrument": "sampled Linux /proc/PID/status VmRSS; direct server only; bytes",
        "sample_interval_ms": 20, "settle_ms": SETTLE.as_millis(), "session_deadline_ms": 180_000,
        "cache_regime": "fresh private disk cache per surface; reused by resident and references; new reference process per operation",
        "limitations": ["Not heap or aggregate descendant memory", "Sampled peaks may miss between-sample peaks", "Fresh reference processes warm the shared disk cache between operations", "Fixed small workload does not establish large-workspace budgets", "Transport teardown/reopen is exercised; request cancellation and interrupted refresh supersession are not established"],
        "configuration": {"base": "fixture base_sha", "lsp_check_mode": "instant", "lsp_diagnostic_profile": "full", "mcp_source": "worktree", "cache_capacity": "analyzer default; no capacity override"},
        "concurrency": "one resident subject; one fresh reference after each resident sample window; unrelated host work is not inventoried",
        "cpu": "not measured; elapsed wall time is not CPU",
        "surfaces": [],
    });
    let outcome = (|| {
        let fixture = Fixture::create(&evidence.join("fixture"), deadline)?;
        report["base_sha"] = json!(fixture.base);
        report["fixture_files_sha256"] = json!(fixture.identity()?);
        let control = retention_control(&env::current_exe().map_err(|e| e.to_string())?, deadline)?;
        report["retained_state_negative_control"] = control;
        report["owned_cleanup_control"] =
            transport::cleanup_control(&env::current_exe().map_err(|e| e.to_string())?, deadline)?;
        for wire in [Wire::Lsp, Wire::Mcp] {
            fixture.edit(0)?;
            let cache = evidence.join(format!("{}-cache", wire.name()));
            fs::create_dir(&cache).map_err(|e| e.to_string())?;
            let surface = measure_surface(&binary, &fixture, &cache, wire, deadline, &evidence)?;
            report["surfaces"]
                .as_array_mut()
                .ok_or("missing surfaces")?
                .push(surface);
        }
        if file_digest(&binary)? != before {
            return Err("analyzer bytes changed during observation".into());
        }
        Ok(())
    })();
    report["status"] = json!(if outcome.is_ok() {
        "observed"
    } else {
        "failed"
    });
    if let Err(error) = &outcome {
        report["error"] = json!(error);
    }
    fs::write(
        evidence.join("resident-session.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    outcome
}

fn file_digest(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

struct Fixture {
    root: PathBuf,
    base: String,
}
impl Fixture {
    fn create(root: &Path, deadline: Instant) -> Result<Self, String> {
        fs::create_dir(root)
            .and_then(|()| fs::create_dir(root.join("src")))
            .and_then(|()| fs::create_dir(root.join("tests")))
            .map_err(|e| e.to_string())?;
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"resident_fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        fs::write(root.join("src/lib.rs"), BASE_SOURCE).map_err(|e| e.to_string())?;
        fs::write(root.join("tests/boundary.rs"), TEST_SOURCE).map_err(|e| e.to_string())?;
        transport::git(root, &["init", "-q", "-b", "main"], deadline)?;
        transport::git(
            root,
            &["add", "Cargo.toml", "src/lib.rs", "tests/boundary.rs"],
            deadline,
        )?;
        transport::git(
            root,
            &[
                "-c",
                "user.name=resident fixture",
                "-c",
                "user.email=fixture@ripr.invalid",
                "-c",
                "commit.gpgSign=false",
                "commit",
                "-qm",
                "base",
            ],
            deadline,
        )?;
        let base = transport::git(root, &["rev-parse", "HEAD"], deadline)?
            .trim()
            .to_string();
        if base.len() != 40 || !base.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("fixture base is not a full Git SHA".into());
        }
        Ok(Self {
            root: root.to_path_buf(),
            base,
        })
    }
    fn edit(&self, state: usize) -> Result<(), String> {
        let (source, test) = fixture_contents(state);
        fs::write(self.root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        fs::write(self.root.join("tests/boundary.rs"), test).map_err(|e| e.to_string())
    }
    fn identity(&self) -> Result<Value, String> {
        ["Cargo.toml", "src/lib.rs", "tests/boundary.rs"]
            .into_iter()
            .map(|path| Ok((path.to_string(), json!(file_digest(&self.root.join(path))?))))
            .collect::<Result<serde_json::Map<String, Value>, String>>()
            .map(Value::Object)
    }
}

fn fixture_contents(state: usize) -> (String, String) {
    // The test-only state preserves the immediately preceding production edit.
    let replacement = if state == 1 || state == 2 {
        "amount > limit + 1"
    } else {
        "amount > limit"
    };
    let test = if state == 2 {
        format!(
            "{TEST_SOURCE}#[test]\nfn boundary() {{ assert_eq!(discounted_total(5, 5), 5); }}\n"
        )
    } else {
        TEST_SOURCE.to_string()
    };
    (BASE_SOURCE.replace("amount >= limit", replacement), test)
}

fn start(
    binary: &Path,
    fixture: &Fixture,
    cache: &Path,
    wire: Wire,
    deadline: Instant,
) -> Result<Session, String> {
    let mut session = Session::start(binary, &fixture.root, cache, wire, deadline)?;
    let root = transport::file_uri(&fixture.root);
    if wire == Wire::Lsp {
        session.call("initialize", json!({"processId": std::process::id(), "rootUri": root, "workspaceFolders": [{"uri": root, "name": "fixture"}], "capabilities": {"workspace": {"configuration": true}, "textDocument": {"diagnostic": {"dynamicRegistration": false}}}, "initializationOptions": {"baseRef": fixture.base, "checkMode": "instant", "diagnosticProfile": "full"}}))?;
        session.notify("initialized", json!({}))?;
    } else {
        let init = session.call("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"ripr-resident-session","version":"1"}}))?;
        if init["protocolVersion"] != "2025-11-25" {
            return Err("MCP protocol negotiation differed".into());
        }
        session.notify("notifications/initialized", json!({}))?;
    }
    Ok(session)
}

fn snapshot(session: &mut Session, fixture: &Fixture) -> Result<(Value, Value), String> {
    if session.wire == Wire::Lsp {
        session.call(
            "workspace/executeCommand",
            json!({"command":"ripr.refresh","arguments":[]}),
        )?;
        let status = session.call(
            "workspace/executeCommand",
            json!({"command":"ripr.collectWorkspaceStatus","arguments":[]}),
        )?;
        let snapshot = text(&status, "/analysis_status/snapshot_id")?.to_string();
        if status
            .pointer("/analysis_status/state")
            .and_then(Value::as_str)
            != Some("succeeded")
        {
            return Err("LSP refresh did not finish successfully".into());
        }
        let mut listed = session.call("ripr/listActionableItems", json!({}))?;
        admit_lsp(&listed, &snapshot)?;
        let mut diagnostics = session.call(
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":transport::file_uri(&fixture.root.join("src/lib.rs"))}}),
        )?;
        if diagnostics["kind"] != "full" || !diagnostics["items"].is_array() {
            return Err(format!(
                "LSP did not return full document diagnostics: {diagnostics}"
            ));
        }
        let mut raw = listed.clone();
        raw["document_diagnostics"] = diagnostics.clone();
        // The complete-evidence digest includes seam generation data. Check its
        // agreement with this current snapshot's status before normalizing it;
        // preserve both raw responses, and compare complete diagnostic payloads.
        let complete = normalize_lsp_evidence_identity(&mut listed, &status)?;
        replace_generation(&mut listed, &snapshot);
        // resultId is a pull-transport token, not a finding identity.
        diagnostics
            .as_object_mut()
            .ok_or("diagnostics object missing")?
            .remove("resultId");
        replace_generation(&mut diagnostics, &snapshot);
        replace_generation(&mut diagnostics, &complete);
        listed["document_diagnostics"] = diagnostics;
        Ok((listed, raw))
    } else {
        let refresh = session.tool(
            "ripr_refresh",
            json!({"base":fixture.base,"source":"worktree"}),
        )?;
        if refresh.pointer("/attempt/state").and_then(Value::as_str) != Some("completed")
            || refresh["fresh_for_working_tree"] != true
        {
            return Err("MCP refresh is not completed and current".into());
        }
        let snapshot = text(&refresh, "/snapshot/snapshot_id")?;
        if refresh
            .pointer("/last_known_good/snapshot_id")
            .and_then(Value::as_str)
            != Some(snapshot)
        {
            return Err("MCP last-known-good binding differs".into());
        }
        let status = session.tool("ripr_workspace_status", json!({}))?;
        if status
            .pointer("/session/last_completed_snapshot/snapshot_id")
            .and_then(Value::as_str)
            != Some(snapshot)
        {
            return Err("MCP status snapshot binding differs".into());
        }
        let mut listed = session.tool("ripr_list_gaps", json!({"snapshot_id":snapshot}))?;
        admit_mcp(&listed, snapshot)?;
        let items = listed["items"]
            .as_array()
            .ok_or("MCP missing selected items")?
            .clone();
        if items.len() > 16 {
            return Err("MCP fixture exceeds 16-item detail bound".into());
        }
        let mut evidence = Vec::new();
        for item in items {
            let canonical = text(&item, "/canonical_id")?;
            let detail = session.tool(
                "ripr_get_gap",
                json!({"canonical_id":canonical,"snapshot_id":snapshot}),
            )?;
            if detail["snapshot_id"] != snapshot
                || detail.pointer("/item/canonical_id").and_then(Value::as_str) != Some(canonical)
            {
                return Err("MCP detailed evidence snapshot/item binding differs".into());
            }
            evidence.push(detail);
        }
        listed["evidence"] = json!(evidence);
        let raw = listed.clone();
        replace_generation(&mut listed, snapshot);
        Ok((listed, raw))
    }
}

fn normalize_lsp_evidence_identity(listed: &mut Value, status: &Value) -> Result<String, String> {
    let complete = text(listed, "/complete_evidence_identity")?.to_string();
    if status
        .pointer("/diagnostic_budget/complete_evidence_identity")
        .and_then(Value::as_str)
        != Some(complete.as_str())
    {
        return Err("LSP complete-evidence status/list binding differs".into());
    }
    replace_generation(listed, &complete);
    Ok(complete)
}

fn text<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing nonempty {pointer}"))
}
fn admit_lsp(listed: &Value, snapshot: &str) -> Result<(), String> {
    if listed["status"] != "ok"
        || listed["kind"] != "actionable_items"
        || listed["snapshot_id"] != snapshot
        || listed.get("error").is_some()
    {
        return Err("LSP actionable snapshot binding failed".into());
    }
    if listed["omitted_truncated"] != false || listed["withheld_unknown_truncated"] != false {
        return Err("LSP small fixture list was truncated or undisclosed".into());
    }
    for (items, count) in [
        ("selected", "selected_count"),
        ("omitted", "omitted_count"),
        ("hidden_gaps", "hidden_gap_count"),
        ("withheld_unknown_findings", "withheld_unknown_count"),
    ] {
        let actual = listed[items]
            .as_array()
            .ok_or_else(|| format!("LSP missing {items}"))?
            .len() as u64;
        if listed[count].as_u64() != Some(actual) {
            return Err(format!("LSP incomplete {items} population"));
        }
    }
    let selected = listed["selected_count"]
        .as_u64()
        .ok_or("missing selected_count")?;
    let omitted = listed["omitted_count"]
        .as_u64()
        .ok_or("missing omitted_count")?;
    if listed["total_count"].as_u64() != selected.checked_add(omitted) {
        return Err("LSP total population differs from selected plus omitted".into());
    }
    Ok(())
}
fn admit_mcp(listed: &Value, snapshot: &str) -> Result<(), String> {
    if listed["snapshot_id"] != snapshot || listed.get("error").is_some() {
        return Err("MCP list binding failed".into());
    }
    // The two independent pagination cursors must both be exhausted.
    for pointer in ["/page/next_offset", "/omitted_page/next_offset"] {
        if listed.pointer(pointer) != Some(&Value::Null) {
            return Err(format!(
                "MCP small fixture is paginated or missing {pointer}"
            ));
        }
    }
    for (items, count) in [("items", "filtered"), ("omitted_items", "omitted")] {
        let actual = listed[items]
            .as_array()
            .ok_or_else(|| format!("MCP missing {items}"))?
            .len() as u64;
        if listed[count].as_u64() != Some(actual) {
            return Err(format!("MCP incomplete {items} population"));
        }
    }
    // Inline-detail suppression is recoverable through mandatory full get_gap
    // calls. Any item omission, other overflow or undisclosed state refuses.
    if listed["omitted"] != 0 || listed["total"] != listed["filtered"] {
        return Err("MCP fixture did not retain its complete item population".into());
    }
    let inline_only = listed["overflowed"] == true
        && listed["overflow_reasons"] == json!(["inline_detail_limit"]);
    let no_overflow = listed["overflowed"] == false && listed["overflow_reasons"] == json!([]);
    if !inline_only && !no_overflow {
        return Err(format!(
            "MCP small fixture selection overflowed or was undisclosed: {listed}"
        ));
    }
    Ok(())
}
fn replace_generation(value: &mut Value, generation: &str) {
    match value {
        Value::String(text) if text == generation => {
            *text = "<validated-session-generation>".into()
        }
        Value::String(text) if text == &format!("ripr://snapshot/{generation}") => {
            *text = "ripr://snapshot/<validated-session-generation>".into()
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| replace_generation(item, generation)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|item| replace_generation(item, generation)),
        _ => {}
    }
}
fn require_parity(resident: &Value, fresh: &Value) -> Result<(), String> {
    if resident != fresh {
        Err("resident snapshot differs from fresh same-state analysis".into())
    } else {
        Ok(())
    }
}
const SEMANTIC_COLLECTIONS: [&str; 8] = [
    "selected",
    "omitted",
    "hidden_gaps",
    "withheld_unknown_findings",
    "document_diagnostics",
    "items",
    "omitted_items",
    "evidence",
];
fn semantic_projection(value: &Value) -> Value {
    Value::Object(
        SEMANTIC_COLLECTIONS
            .into_iter()
            .filter_map(|key| value.get(key).map(|v| (key.to_string(), v.clone())))
            .collect(),
    )
}
fn require_changed(before: &Value, after: &Value) -> Result<(), String> {
    // Input stamps hash raw diffs; they cannot establish changed findings.
    if semantic_projection(before) == semantic_projection(after) {
        Err("fixture did not establish different semantic states".into())
    } else {
        Ok(())
    }
}
fn stale_with_current_metadata(previous: &Value, current: &Value) -> Value {
    let mut stale = current.clone();
    for key in SEMANTIC_COLLECTIONS {
        if let Some(value) = previous.get(key) {
            stale[key] = value.clone();
        }
    }
    stale
}

fn measure_surface(
    binary: &Path,
    fixture: &Fixture,
    cache: &Path,
    wire: Wire,
    deadline: Instant,
    evidence: &Path,
) -> Result<Value, String> {
    let mut resident = start(binary, fixture, cache, wire, deadline)?;
    let mut rows = Vec::new();
    let mut initial = None;
    let mut previous = None;
    for (index, (operation, state)) in [
        ("initial_load", 0),
        ("unchanged_request", 0),
        ("production_edit", 1),
        ("test_only_edit", 2),
        ("restore_initial", 0),
        ("second_production_edit", 1),
        ("second_restore_initial", 0),
        ("process_restart", 0),
        ("idle_settle", 0),
    ]
    .into_iter()
    .enumerate()
    {
        fixture.edit(state)?;
        if operation == "process_restart" {
            resident.finish()?;
            resident = start(binary, fixture, cache, wire, deadline)?;
        }
        if index != 0 && operation != "process_restart" {
            resident.begin_observation();
        }
        let (observed, observed_raw) = snapshot(&mut resident, fixture)?;
        let memory = resident.settle(if operation == "idle_settle" {
            Duration::from_secs(1)
        } else {
            SETTLE
        })?;
        // Reference cost is not charged to the resident operation's samples.
        let mut fresh = start(binary, fixture, cache, wire, deadline)?;
        let (expected, expected_raw) = snapshot(&mut fresh, fixture)?;
        fresh.finish()?;
        // Preserve both documents before adjudication, including a failed
        // parity observation. Rehearsal failures are evidence, not lost rows.
        let operation_receipt = json!({"operation":operation,"fixture_identity":fixture.identity()?,"pid":resident.id(),"memory":memory,"resident":observed,"fresh":expected,"resident_raw":observed_raw,"fresh_raw":expected_raw});
        fs::write(
            evidence.join(format!("{}-{index:02}-{operation}.json", wire.name())),
            serde_json::to_vec_pretty(&operation_receipt).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        require_parity(&observed, &expected)?;
        if index == 0 {
            let count = if wire == Wire::Lsp {
                observed["total_count"].as_u64()
            } else {
                observed["items"].as_array().map(|items| items.len() as u64)
            };
            if count.is_none_or(|count| count == 0) {
                return Err("fixture selected no initial semantic subjects".into());
            }
            initial = Some(observed.clone());
        }
        if operation == "production_edit" {
            let previous = initial.as_ref().ok_or("no initial snapshot")?;
            require_changed(previous, &observed)?;
            // Replay prior semantics despite a successful new refresh: same
            // verdict function must reject the stale implementation.
            if require_parity(&stale_with_current_metadata(previous, &expected), &expected).is_ok()
            {
                return Err("stale-result negative control did not fail".into());
            }
        }
        if operation == "test_only_edit" {
            require_changed(
                previous.as_ref().ok_or("no preceding production state")?,
                &observed,
            )?;
        }
        previous = Some(observed.clone());
        let initial_retained = rows
            .first()
            .and_then(|row: &Value| row.pointer("/memory/retained_rss_bytes"))
            .and_then(Value::as_u64);
        let retained_delta = memory["retained_rss_bytes"]
            .as_u64()
            .zip(initial_retained)
            .map(|(now, initial)| i128::from(now) - i128::from(initial));
        rows.push(json!({"operation":operation,"fixture_identity":fixture.identity()?,"pid":resident.id(),"memory":memory,"retained_delta_from_initial_bytes":retained_delta,"parity":"equal","snapshot":observed}));
    }
    resident.finish()?;
    let growth = rows
        .get(6)
        .and_then(|row| row.pointer("/memory/retained_rss_bytes"))
        .and_then(Value::as_u64)
        .zip(
            rows.first()
                .and_then(|row| row.pointer("/memory/retained_rss_bytes"))
                .and_then(Value::as_u64),
        )
        .map(|(last, first)| i128::from(last) - i128::from(first));
    Ok(
        json!({"surface":wire.name(),"same_state_retained_growth_bytes":growth,"rows":rows,"stale_result_negative_control":"rejected","cleanup":"owned process group signaled and primary reaped; no escaped-descendant claim"}),
    )
}

fn retention_control(executable: &Path, deadline: Instant) -> Result<Value, String> {
    let mut child = Session::control(executable, deadline)?;
    child.begin_observation();
    child.call("hold", json!({"bytes":0}))?;
    let baseline = child.settle(SETTLE)?;
    child.begin_observation();
    child.call("hold", json!({"bytes":32 * 1024 * 1024}))?;
    let retained = child.settle(SETTLE)?;
    let before = baseline["retained_rss_bytes"]
        .as_u64()
        .ok_or("missing control baseline RSS")?;
    let after = retained["retained_rss_bytes"]
        .as_u64()
        .ok_or("missing control retained RSS")?;
    let rejected = retained_growth(before, after, 16 * 1024 * 1024).is_err();
    child.finish()?;
    if !rejected {
        return Err("measured-child retained-state negative control was not detected".into());
    }
    Ok(
        json!({"status":"rejected_as_expected","scope":"separate synthetic measured child, not RIPR","baseline":baseline,"retained":retained,"threshold_bytes":16*1024*1024}),
    )
}
fn retained_growth(before: u64, after: u64, allowed: u64) -> Result<(), String> {
    if after.saturating_sub(before) > allowed {
        Err("retained RSS growth exceeds the selected control threshold".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_semantics_fail_even_when_counts_match() {
        let a = json!({"items":[{"id":"a","line":3}],"count":1});
        let b = json!({"items":[{"id":"b","line":3}],"count":1});
        assert!(require_parity(&a, &b).is_err());
        assert!(require_changed(&a, &b).is_ok());
    }
    #[test]
    fn generation_only_change_is_not_a_semantic_fixture() {
        let a = json!({"snapshot_id":"one","items":[{"id":"same"}]});
        let b = json!({"snapshot_id":"two","items":[{"id":"same"}]});
        assert!(require_changed(&a, &b).is_err());
    }
    #[test]
    fn metadata_only_change_is_not_a_semantic_fixture() {
        let a = json!({"analysis_outcome":{"identity":"one"},"items":[{"id":"same"}]});
        let b = json!({"analysis_outcome":{"identity":"two"},"items":[{"id":"same"}]});
        assert!(require_changed(&a, &b).is_err());
        let c = json!({"analysis_outcome":{"identity":"two"},"items":[{"id":"changed"}]});
        let stale = stale_with_current_metadata(&a, &c);
        assert_eq!(stale["analysis_outcome"], c["analysis_outcome"]);
        assert!(require_parity(&stale, &c).is_err());
    }
    #[test]
    fn test_only_edit_keeps_the_preceding_production_bytes() {
        let a = fixture_contents(0);
        let b = fixture_contents(1);
        let c = fixture_contents(2);
        assert_ne!(a.0, b.0);
        assert_eq!(b.0, c.0);
        assert_ne!(b.1, c.1);
        assert_eq!(a, fixture_contents(0));
    }
    #[test]
    fn retains_semantic_ids_during_generation_normalization() {
        let mut value = json!({"snapshot_id":"snapshot:4","items":[{"id":"gap:4","version":"1"}]});
        replace_generation(&mut value, "snapshot:4");
        assert_eq!(value["items"][0]["id"], "gap:4");
        assert_eq!(value["items"][0]["version"], "1");
    }
    #[test]
    fn lsp_evidence_digest_requires_current_status_agreement() {
        let mut listed = json!({"complete_evidence_identity":"digest:a","items":[{"id":"keep"}]});
        let wrong = json!({"diagnostic_budget":{"complete_evidence_identity":"digest:b"}});
        assert!(normalize_lsp_evidence_identity(&mut listed, &wrong).is_err());
        assert_eq!(listed["complete_evidence_identity"], "digest:a");
        let current = json!({"diagnostic_budget":{"complete_evidence_identity":"digest:a"}});
        assert!(normalize_lsp_evidence_identity(&mut listed, &current).is_ok());
        assert_eq!(listed["items"][0]["id"], "keep");
    }
    #[test]
    fn snapshot_links_do_not_supply_semantic_variation() {
        let mut a = json!({"evidence":[{"snapshot_id":"one","item":{"canonical_id":"keep","links":{"snapshot":"ripr://snapshot/one"}}}]});
        let mut b = json!({"evidence":[{"snapshot_id":"two","item":{"canonical_id":"keep","links":{"snapshot":"ripr://snapshot/two"}}}]});
        replace_generation(&mut a, "one");
        replace_generation(&mut b, "two");
        assert!(require_changed(&a, &b).is_err());
        assert!(require_parity(&a, &b).is_ok());
    }
    #[test]
    fn retained_growth_control_uses_current_memory_not_lifetime_peak() {
        assert!(retained_growth(100, 120, 20).is_ok());
        assert!(retained_growth(100, 121, 20).is_err());
        assert!(retained_growth(120, 100, 0).is_ok());
    }
    #[test]
    fn incomplete_lists_and_wrong_binding_refuse() {
        assert!(admit_lsp(&json!({}), "s").is_err());
        assert!(admit_mcp(&json!({"snapshot_id":"s","items":[]}), "s").is_err());
        let good = json!({"snapshot_id":"s","items":[{}],"filtered":1,"total":1,"omitted_items":[],"omitted":0,"overflowed":false,"overflow_reasons":[],"page":{"next_offset":null},"omitted_page":{"next_offset":null}});
        assert!(admit_mcp(&good, "s").is_ok());
        assert!(admit_mcp(&good, "other").is_err());
        let mut inline_only = good.clone();
        inline_only["overflowed"] = json!(true);
        inline_only["overflow_reasons"] = json!(["inline_detail_limit"]);
        assert!(admit_mcp(&inline_only, "s").is_ok());
        inline_only["overflow_reasons"] = json!(["serialized_byte_limit"]);
        assert!(admit_mcp(&inline_only, "s").is_err());
        let mut partial = good;
        partial["omitted_page"]["next_offset"] = json!(1);
        assert!(admit_mcp(&partial, "s").is_err());
    }
}
