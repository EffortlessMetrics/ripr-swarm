//! A long-lived editor session against a fresh one, over the real wire.
//!
//! The server keeps state between refreshes: the committed snapshot, the
//! refresh generation, the bytes it consumed, and the on-disk caches. None of
//! that may change what it reports. Each case edits a workspace outside the
//! editor, tells a running session through `workspace/didChangeWatchedFiles`,
//! and compares what that session serves with what a server started
//! afterwards on the same bytes, with its own empty cache directory, serves.
//!
//! The oracle is the pull report (`workspace/diagnostic`): it is the complete
//! served set of the committed snapshot, so no publish has to be awaited or
//! counted. Members that name the refresh attempt are excluded from the
//! comparison, by name: `resultId`, `snapshot_id`, `generated_at`, and the two
//! identities derived from the attempt, `snapshot_profile_budget_identity`
//! and `complete_evidence_identity`. The budget outcome itself is compared
//! through its counts and byte totals in the workspace status.
//!
//! The cases pin three ways the two sessions used to disagree:
//!
//! 1. the byte budget counted the `snapshot:<generation>` text stamped into
//!    every seam diagnostic, so a session ten refreshes old served one
//!    diagnostic fewer than a fresh one;
//! 2. a full refresh ordered the finding on the changed line after the
//!    repository seam inventory and cut it at the byte limit, while the
//!    interactive refresh of the same workspace served it;
//! 3. a clean Python or TypeScript file rewritten outside the editor
//!    scheduled no refresh, so the session kept its clean result while a
//!    fresh one reported the gap.
//!
//! Static classes only; no runtime mutation outcome is claimed.

use super::{
    ANALYSIS_TIMEOUT, CompatFixtureRoot, LspSession, editor_file_uri, execute_compat_command,
    expect_result, run_compat_git, shutdown_exit_and_wait, unique_compat_fixture_root,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Enough unexercised comparisons that the seam inventory alone overflows the
/// default 64 KiB delivery budget.
const SEAM_MODULES: usize = 72;

struct Workspace {
    owned: CompatFixtureRoot,
    next_cache: u32,
}

impl Workspace {
    fn root(&self) -> PathBuf {
        self.owned.path.join("repo")
    }

    /// A cache directory no session has used.
    fn fresh_cache(&mut self) -> Result<String, String> {
        self.next_cache += 1;
        let path = self.owned.path.join(format!("cache-{}", self.next_cache));
        path.to_str()
            .map(str::to_string)
            .ok_or_else(|| format!("cache path is not UTF-8: {}", path.display()))
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let path = self.root().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create {} failed: {err}", parent.display()))?;
        }
        std::fs::write(&path, content).map_err(|err| format!("write {relative} failed: {err}"))
    }

    fn read(&self, relative: &str) -> Result<String, String> {
        std::fs::read_to_string(self.root().join(relative))
            .map_err(|err| format!("read {relative} failed: {err}"))
    }

    fn uri(&self, relative: &str) -> Result<String, String> {
        editor_file_uri(&self.root().join(relative))
    }

    fn commit_base(&self) -> Result<(), String> {
        let root = self.root();
        run_compat_git(&root, &["init", "-q", "-b", "main"])?;
        run_compat_git(&root, &["config", "user.email", "ripr@example.invalid"])?;
        run_compat_git(&root, &["config", "user.name", "RIPR Test"])?;
        run_compat_git(&root, &["config", "core.autocrlf", "false"])?;
        std::fs::write(root.join(".git/info/exclude"), "/target/\n")
            .map_err(|err| format!("write git exclude failed: {err}"))?;
        run_compat_git(&root, &["add", "-A"])?;
        run_compat_git(&root, &["commit", "-q", "-m", "base"])
    }
}

fn module_source(index: usize, operator: &str) -> String {
    format!(
        "pub fn gate{index}(amount: u64) -> bool {{\n    amount {operator} {}\n}}\n",
        100 + index
    )
}

/// A crate whose seam inventory is larger than the delivery budget: one
/// comparison per module, and a single test that pins only the first.
fn seam_heavy_rust_workspace(label: &str) -> Result<Workspace, String> {
    let workspace = Workspace {
        owned: unique_compat_fixture_root(label)?,
        next_cache: 0,
    };
    let mut lib = String::new();
    for index in 0..SEAM_MODULES {
        lib.push_str(&format!("mod part{index};\npub use part{index}::*;\n"));
        workspace.write(&format!("src/part{index}.rs"), &module_source(index, ">"))?;
    }
    workspace.write("src/lib.rs", &lib)?;
    workspace.write(
        "tests/case.rs",
        "use lsp_differential_subject::*;\n\n#[test]\nfn gate0_far_from_the_boundary() {\n    assert!(gate0(900));\n}\n",
    )?;
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"lsp-differential-subject\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )?;
    workspace.commit_base()?;
    Ok(workspace)
}

/// A copy of a shipped example project, committed as its own base.
fn example_workspace(example: &str) -> Result<Workspace, String> {
    fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
        std::fs::create_dir_all(to)
            .map_err(|err| format!("create {} failed: {err}", to.display()))?;
        let entries = std::fs::read_dir(from)
            .map_err(|err| format!("read {} failed: {err}", from.display()))?;
        for entry in entries {
            let entry = entry.map_err(|err| format!("read {} failed: {err}", from.display()))?;
            let kind = entry
                .file_type()
                .map_err(|err| format!("stat {:?} failed: {err}", entry.path()))?;
            if kind.is_dir() {
                copy_tree(&entry.path(), &to.join(entry.file_name()))?;
            } else if kind.is_file() {
                std::fs::copy(entry.path(), to.join(entry.file_name()))
                    .map_err(|err| format!("copy {:?} failed: {err}", entry.path()))?;
            }
        }
        Ok(())
    }
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(example)
        .join("project");
    if !project.is_dir() {
        return Err(format!("example project {} is missing", project.display()));
    }
    let workspace = Workspace {
        owned: unique_compat_fixture_root(example)?,
        next_cache: 0,
    };
    copy_tree(&project, &workspace.root())?;
    workspace.commit_base()?;
    Ok(workspace)
}

/// A pull-diagnostics session on `root` with its own cache directory.
fn start_session(root: &Path, cache: &str) -> Result<LspSession, String> {
    let mut session = LspSession::spawn_with_env(&[("RIPR_CACHE_DIR", cache)])?;
    let initialize = session.request(
        "initialize",
        json!({
            "processId": null,
            "rootUri": editor_file_uri(root)?,
            "initializationOptions": {
                "baseRef": "HEAD",
                "checkMode": "draft",
                "diagnosticProfile": "full"
            },
            "capabilities": { "textDocument": { "diagnostic": {} } },
        }),
    )?;
    expect_result(&initialize, "initialize")?;
    session.notify("initialized", Some(json!({})))?;
    Ok(session)
}

fn status(session: &mut LspSession) -> Result<Value, String> {
    execute_compat_command(session, "ripr.collectWorkspaceStatus")
}

fn status_text<'a>(status: &'a Value, pointer: &str) -> &'a str {
    status
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// Run the explicit full refresh and require the committed snapshot to be the
/// full one it produced.
fn full_refresh(session: &mut LspSession) -> Result<Value, String> {
    let response = session.request_with_timeout(
        "workspace/executeCommand",
        json!({"command": "ripr.refresh", "arguments": []}),
        ANALYSIS_TIMEOUT,
    )?;
    expect_result(&response, "ripr.refresh")?;
    let status = status(session)?;
    if status_text(&status, "/run_status") != "full"
        || status_text(&status, "/analysis_status/reason") != "explicit_refresh"
    {
        return Err(format!(
            "fixture: the explicit refresh must commit a full snapshot, status: {status}"
        ));
    }
    Ok(status)
}

/// Poll the status until a refresh other than `previous_attempt` has
/// committed for `reason`. Used for refreshes no request answers.
fn await_refresh(
    session: &mut LspSession,
    previous_attempt: &str,
    reason: &str,
) -> Result<Value, String> {
    let deadline = Instant::now() + ANALYSIS_TIMEOUT;
    loop {
        let status = status(session)?;
        let attempt = status_text(&status, "/analysis_status/attempt_id");
        let settled = status.pointer("/analysis_status/pending") == Some(&Value::Bool(false))
            && status_text(&status, "/analysis_status/state") == "succeeded";
        if settled && attempt != previous_attempt {
            if status_text(&status, "/analysis_status/reason") != reason {
                return Err(format!(
                    "expected a refresh for `{reason}` after attempt {previous_attempt}, status: {status}"
                ));
            }
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no refresh for `{reason}` committed after attempt {previous_attempt} within {ANALYSIS_TIMEOUT:?}; status: {status}"
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn notify_changed(session: &mut LspSession, uris: &[String]) -> Result<(), String> {
    let changes: Vec<Value> = uris
        .iter()
        .map(|uri| json!({"uri": uri, "type": 2}))
        .collect();
    session.notify(
        "workspace/didChangeWatchedFiles",
        Some(json!({ "changes": changes })),
    )
}

fn strip_attempt_identity(value: &mut Value) {
    match value {
        Value::Object(members) => {
            for name in [
                "resultId",
                "snapshot_id",
                "complete_evidence_identity",
                "generated_at",
                "snapshot_profile_budget_identity",
            ] {
                members.remove(name);
            }
            members.values_mut().for_each(strip_attempt_identity);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_attempt_identity),
        _ => {}
    }
}

/// The served diagnostics of the committed snapshot, by document, with the
/// attempt-naming members removed and documents without diagnostics dropped.
fn served(session: &mut LspSession) -> Result<Vec<(String, Vec<Value>)>, String> {
    let response = session.request_with_timeout(
        "workspace/diagnostic",
        json!({"previousResultIds": []}),
        ANALYSIS_TIMEOUT,
    )?;
    let items = expect_result(&response, "workspace/diagnostic")?
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("workspace/diagnostic carries no items: {response}"))?;
    let mut documents = Vec::new();
    for item in items {
        let uri = item
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("workspace report item has no uri: {item}"))?;
        let mut diagnostics = item
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        diagnostics.iter_mut().for_each(strip_attempt_identity);
        if !diagnostics.is_empty() {
            documents.push((uri.to_string(), diagnostics));
        }
    }
    documents.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(documents)
}

fn is_finding(diagnostic: &Value) -> bool {
    diagnostic
        .pointer("/data/finding_id")
        .is_some_and(Value::is_string)
        && diagnostic.pointer("/data/seam_id").is_none()
}

fn is_seam(diagnostic: &Value) -> bool {
    diagnostic
        .pointer("/data/seam_id")
        .is_some_and(Value::is_string)
}

fn count(served: &[(String, Vec<Value>)], keep: fn(&Value) -> bool) -> usize {
    served
        .iter()
        .flat_map(|(_, diagnostics)| diagnostics)
        .filter(|diagnostic| keep(diagnostic))
        .count()
}

/// The first document or diagnostic on which two served sets differ.
fn first_difference(
    fresh: &[(String, Vec<Value>)],
    aged: &[(String, Vec<Value>)],
) -> Option<String> {
    let summary = |diagnostics: &[Value]| {
        diagnostics
            .iter()
            .map(|diagnostic| {
                diagnostic
                    .pointer("/data/diagnostic_id")
                    .or_else(|| diagnostic.get("code"))
                    .map_or_else(|| "?".to_string(), Value::to_string)
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    for index in 0..fresh.len().max(aged.len()) {
        match (fresh.get(index), aged.get(index)) {
            (Some(left), Some(right)) if left == right => {}
            (left, right) => {
                // Name the first diagnostic that differs when the two
                // documents list the same ones.
                let pair = left.zip(right).and_then(|((_, fresh), (_, aged))| {
                    fresh
                        .iter()
                        .zip(aged)
                        .find(|(fresh, aged)| fresh != aged)
                        .map(|(fresh, aged)| {
                            format!("; first differing pair: {fresh} against {aged}")
                        })
                });
                return Some(format!(
                    "fresh session serves {:?} [{}], long-lived session serves {:?} [{}]{}",
                    left.map(|(uri, _)| uri),
                    left.map_or_else(String::new, |(_, diagnostics)| summary(diagnostics)),
                    right.map(|(uri, _)| uri),
                    right.map_or_else(String::new, |(_, diagnostics)| summary(diagnostics)),
                    pair.unwrap_or_default(),
                ));
            }
        }
    }
    None
}

fn refresh_generation(status: &Value) -> Result<u64, String> {
    status_text(status, "/analysis_status/last_success_snapshot_id")
        .strip_prefix("snapshot:")
        .and_then(|generation| generation.parse().ok())
        .ok_or_else(|| format!("status names no snapshot generation: {status}"))
}

/// Cases 1 and 2. A session that has refreshed past its tenth generation
/// serves exactly what a fresh session serves, after every external edit, and
/// the finding on the changed line is in both.
#[test]
fn long_lived_session_serves_what_a_fresh_session_serves() -> Result<(), String> {
    let mut workspace = seam_heavy_rust_workspace("differential-aged")?;
    let root = workspace.root();
    let aged_cache = workspace.fresh_cache()?;
    let mut aged = start_session(&root, &aged_cache)?;
    let mut aged_status = full_refresh(&mut aged)?;
    let mut compared = 0usize;
    let mut step = 0usize;
    // Until the session is past `snapshot:10`, and at least four edits.
    while step < 4 || refresh_generation(&aged_status)? < 10 {
        if step >= SEAM_MODULES {
            return Err(format!(
                "fixture: the session never reached its tenth refresh; status: {aged_status}"
            ));
        }
        // One changed module at a time: the previous one goes back to its
        // committed bytes, so a single finding competes with the inventory.
        let relative = format!("src/part{step}.rs");
        let mut changed = vec![workspace.uri(&relative)?];
        workspace.write(&relative, &module_source(step, ">="))?;
        if let Some(previous) = step.checked_sub(1) {
            let previous_relative = format!("src/part{previous}.rs");
            workspace.write(&previous_relative, &module_source(previous, ">"))?;
            changed.push(workspace.uri(&previous_relative)?);
        }
        let before = status_text(&aged_status, "/analysis_status/attempt_id").to_string();
        notify_changed(&mut aged, &changed)?;
        // The watched-file refresh is interactive and defers the inventory.
        // Let it commit before the full refresh, or it would replace it.
        await_refresh(&mut aged, &before, "watched_input")?;
        aged_status = full_refresh(&mut aged)?;
        let aged_served = served(&mut aged)?;

        let fresh_cache = workspace.fresh_cache()?;
        let mut fresh = start_session(&root, &fresh_cache)?;
        let fresh_status = full_refresh(&mut fresh)?;
        let fresh_served = served(&mut fresh)?;
        shutdown_exit_and_wait(&mut fresh)?;

        // Fixture: the budget is under pressure, so selection is exercised.
        let omitted = fresh_status
            .pointer("/diagnostic_budget/omitted_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if omitted == 0 || count(&fresh_served, is_seam) == 0 {
            return Err(format!(
                "fixture: the seam inventory must overflow the delivery budget, status: {fresh_status}"
            ));
        }
        // Case 2: the finding on the line just changed is served under a
        // full refresh, although the inventory overflows the budget.
        let changed_uri = workspace.uri(&relative)?;
        let finding_served = fresh_served
            .iter()
            .filter(|(uri, _)| super::same_fixture_file_uri(uri, &changed_uri))
            .flat_map(|(_, diagnostics)| diagnostics)
            .any(is_finding);
        if !finding_served {
            return Err(format!(
                "step {step}: a full refresh must serve the finding on the changed line of {relative}; \
                 {} findings and {} seam rows were served, {omitted} diagnostics omitted",
                count(&fresh_served, is_finding),
                count(&fresh_served, is_seam),
            ));
        }
        // Case 1 and the general claim: the two sessions agree exactly.
        if let Some(difference) = first_difference(&fresh_served, &aged_served) {
            return Err(format!(
                "step {step}: a session at {} differs from a fresh session at {}: {difference}",
                status_text(&aged_status, "/analysis_status/last_success_snapshot_id"),
                status_text(&fresh_status, "/analysis_status/last_success_snapshot_id"),
            ));
        }
        for pointer in [
            "/diagnostic_budget/complete_bytes",
            "/diagnostic_budget/selected_bytes",
            "/diagnostic_budget/selected_count",
            "/diagnostic_budget/omitted_count",
            "/diagnostics",
        ] {
            if aged_status.pointer(pointer) != fresh_status.pointer(pointer) {
                return Err(format!(
                    "step {step}: status `{pointer}` is {:?} in the long-lived session and {:?} in a fresh one",
                    aged_status.pointer(pointer),
                    fresh_status.pointer(pointer),
                ));
            }
        }
        compared += 1;
        step += 1;
    }
    if compared < 4 {
        return Err(format!("only {compared} states were compared"));
    }
    shutdown_exit_and_wait(&mut aged)
}

/// Case 3, and its Rust control. A clean source rewritten outside the editor
/// refreshes on the watched-file event alone, in every analyzed language, and
/// the session then serves what a fresh session serves.
#[test]
fn external_rewrite_of_a_clean_source_refreshes_in_every_language() -> Result<(), String> {
    for (example, relative) in [
        ("rust-discount-threshold", "src/lib.rs"),
        ("python-bulk-discount", "pricing/core.py"),
        ("typescript-discount-threshold", "src/pricing.ts"),
    ] {
        let mut workspace = example_workspace(example)?;
        let root = workspace.root();
        let cache = workspace.fresh_cache()?;
        let mut session = start_session(&root, &cache)?;
        let clean = full_refresh(&mut session)?;
        let served_clean = served(&mut session)?;
        if count(&served_clean, is_finding) != 0 {
            return Err(format!(
                "fixture: {example} must start without findings, served {served_clean:?}"
            ));
        }

        let original = workspace.read(relative)?;
        if !original.contains(">=") {
            return Err(format!("fixture: {example}/{relative} has no `>=` to move"));
        }
        workspace.write(relative, &original.replacen(">=", ">", 1))?;
        let before = status_text(&clean, "/analysis_status/attempt_id").to_string();
        // No refresh request follows: the event alone must schedule one.
        notify_changed(&mut session, &[workspace.uri(relative)?])?;
        await_refresh(&mut session, &before, "watched_input").map_err(|err| {
            format!("{example}: an external rewrite of {relative} scheduled no refresh: {err}")
        })?;
        let after_event: Vec<Value> = served(&mut session)?
            .into_iter()
            .flat_map(|(_, diagnostics)| diagnostics)
            .filter(is_finding)
            .collect();

        let fresh_cache = workspace.fresh_cache()?;
        let mut fresh = start_session(&root, &fresh_cache)?;
        full_refresh(&mut fresh)?;
        let fresh_findings: Vec<Value> = served(&mut fresh)?
            .into_iter()
            .flat_map(|(_, diagnostics)| diagnostics)
            .filter(is_finding)
            .collect();
        shutdown_exit_and_wait(&mut fresh)?;

        if fresh_findings.is_empty() {
            return Err(format!(
                "fixture: {example} must report a finding once `>=` becomes `>` in {relative}"
            ));
        }
        // The watched-file refresh defers the seam inventory by design, so
        // the comparison is over the findings of the change. A run that is
        // not full also lowers a warning to information by policy, so
        // severity is outside the comparison.
        let without_severity = |findings: &[Value]| -> Vec<Value> {
            findings
                .iter()
                .cloned()
                .map(|mut finding| {
                    if let Some(members) = finding.as_object_mut() {
                        members.remove("severity");
                    }
                    finding
                })
                .collect()
        };
        if without_severity(&after_event) != without_severity(&fresh_findings) {
            return Err(format!(
                "{example}: after the watched-file event the session serves {} findings, a fresh session serves {}: {after_event:?} against {fresh_findings:?}",
                after_event.len(),
                fresh_findings.len()
            ));
        }
        shutdown_exit_and_wait(&mut session)?;
    }
    Ok(())
}
