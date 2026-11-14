//! `cargo xtask agent-eval` — answer-keyed agent eval (#14298).
//!
//! Two questions an agent using ripr actually has, answered cheaply and
//! repeatably on hermetic scenarios (no network, no model calls):
//!
//! 1. **Tokens to the answer.** Following one route (the `ripr init --agents`
//!    CLI loop, the default human check, `json-terse`, `agent brief`, or the
//!    MCP tool flow), how many tokens does the agent read before every gap
//!    in the scenario's answer key is visible: owning function, changed line
//!    and the facts a test must pin? And how many does the whole route cost?
//! 2. **Usefulness of the loop.** Does `check --format agent` report exactly
//!    the keyed gaps (no misses, no extra gaps), stay silent on a control
//!    change the tests already pin, keep flagging after a weak test, and
//!    finish with `verify=complete` after the reference test? With cargo
//!    available the eval also confirms each key: the reference test passes at
//!    head and fails at base, the weak test passes at both.
//!
//! Definitions (do not change silently; they are what the baseline means):
//! - `bytes`: exact stdout plus stderr bytes of a CLI step (what a shell tool
//!   shows an agent); for MCP, the JSON-RPC response line on the wire.
//! - `tokens`: an estimate, not a tokenizer or provider count. Every run of
//!   ASCII letters/digits/underscore costs ceil(len / 4), every other
//!   non-whitespace character costs 1, whitespace is free. MCP steps count
//!   the model-visible part: the tool list on `tools/list`, the `content`
//!   text on `tools/call`, the instructions on `initialize`.
//! - The checkout root is replaced by `<root>` before measuring, so numbers
//!   do not depend on the scratch path length.
//!
//! Static results stay advisory: a closed loop here means ripr's static
//! verdict moved as expected, not that a test catches a runtime fault.

mod scenarios;

use crate::normalize_path;
use crate::run::{ByteCaptureBudget, capture_bytes_in_dir_with_input_env_budget};
use scenarios::{GapKey, SCENARIOS, Scenario};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SCHEMA_VERSION: &str = "ripr-agent-eval-v1";
const BASELINE_PATH: &str = "benchmarks/agent_eval/baseline.json";
const USAGE: &str = "usage: cargo xtask agent-eval [--ripr <binary>] [--scenario <id>]... [--compare <baseline.json>] [--write-baseline] [--skip-cargo]";
/// A route regresses when its tokens grow by more than `then / 10` (10%)
/// plus `REGRESSION_SLACK_TOKENS` over the baseline.
const REGRESSION_DIVISOR: u64 = 10;
const REGRESSION_SLACK_TOKENS: u64 = 20;
const STEP_TIMEOUT: Duration = Duration::from_mins(5);
const MAX_FOLLOWED_STEPS: usize = 8;
const GIT_IDENTITY: [(&str, &str); 6] = [
    ("GIT_AUTHOR_NAME", "ripr agent eval"),
    ("GIT_AUTHOR_EMAIL", "agent-eval@example.invalid"),
    ("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z"),
    ("GIT_COMMITTER_NAME", "ripr agent eval"),
    ("GIT_COMMITTER_EMAIL", "agent-eval@example.invalid"),
    ("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z"),
];
const ROUTES: [&str; 6] = [
    "cli-agent-loop",
    "cli-agent-find",
    "cli-human",
    "cli-json-terse",
    "agent-brief",
    "mcp",
];

#[derive(Debug, Default)]
struct Options {
    ripr: Option<PathBuf>,
    scenarios: Vec<String>,
    compare: Option<PathBuf>,
    write_baseline: bool,
    skip_cargo: bool,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--ripr" => options.ripr = Some(PathBuf::from(value("--ripr")?)),
            "--scenario" => options.scenarios.push(value("--scenario")?),
            "--compare" => options.compare = Some(PathBuf::from(value("--compare")?)),
            "--write-baseline" => options.write_baseline = true,
            "--skip-cargo" => options.skip_cargo = true,
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    for id in &options.scenarios {
        if !SCENARIOS.iter().any(|scenario| scenario.id == id) {
            let known: Vec<&str> = SCENARIOS.iter().map(|scenario| scenario.id).collect();
            return Err(format!(
                "unknown scenario `{id}`; known: {}",
                known.join(", ")
            ));
        }
    }
    if options.write_baseline && !options.scenarios.is_empty() {
        return Err("--write-baseline records every scenario; drop --scenario".to_string());
    }
    if options.write_baseline && options.skip_cargo {
        // A baseline without the cargo key checks would leave them unguarded.
        return Err("--write-baseline needs the cargo key checks; drop --skip-cargo".to_string());
    }
    if options.write_baseline && options.compare.is_some() {
        // A failed compare must never replace the baseline it failed against.
        return Err("--write-baseline replaces the baseline; drop --compare".to_string());
    }
    Ok(options)
}

/// Estimated tokens; see the module header for the definition.
pub(crate) fn estimate_tokens(text: &str) -> u64 {
    let mut tokens = 0u64;
    let mut word = 0u64;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            word += 1;
            continue;
        }
        tokens += word.div_ceil(4);
        word = 0;
        if !ch.is_whitespace() {
            tokens += 1;
        }
    }
    tokens + word.div_ceil(4)
}

#[derive(Debug, Clone)]
struct Step {
    label: String,
    exit: Option<i32>,
    bytes: u64,
    tokens: u64,
    /// Model-visible normalized text, used for answer detection only.
    text: String,
}

impl Step {
    fn to_json(&self) -> Value {
        json!({
            "step": self.label,
            "exit": self.exit,
            "bytes": self.bytes,
            "tokens": self.tokens,
        })
    }
}

struct Workspace {
    root: PathBuf,
    spellings: Vec<String>,
}

impl Workspace {
    fn normalize(&self, text: &str) -> String {
        let mut out = text.to_string();
        for spelling in &self.spellings {
            out = out.replace(spelling.as_str(), "<root>");
        }
        out
    }
}

fn scratch_root() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("ripr-agent-eval-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| format!("failed to clear {}: {err}", dir.display()))?;
    }
    fs::create_dir_all(&dir).map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    Ok(dir)
}

fn write_file(path: &Path, body: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(path, body).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

struct Captured {
    exit: Option<i32>,
    stdout: String,
    stderr: String,
}

fn capture(
    program: &Path,
    args: &[String],
    cwd: &Path,
    input: Option<&[u8]>,
    envs: &[(&str, &str)],
) -> Result<Captured, String> {
    let budget = ByteCaptureBudget {
        timeout: STEP_TIMEOUT,
        stdout_bytes: 16 * 1024 * 1024,
        stderr_bytes: 4 * 1024 * 1024,
    };
    let context = format!("{} {}", program.display(), args.join(" "));
    // Isolate every scenario child, including analyzer/MCP/Cargo Git descendants.
    // cwd alone does not override inherited repository/index/object routing or
    // Git configuration injection (notably under `git bisect run`).
    let ambient: Vec<String> = std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| {
            let name = name.to_ascii_uppercase();
            name.starts_with("RIPR_") || name.starts_with("GIT_")
        })
        .collect();
    let ambient: Vec<&str> = ambient.iter().map(String::as_str).collect();
    // Removing config selectors must not fall back to caller global/system config.
    // Reapply only trusted scenario settings (identity, Cargo target, MCP tools).
    let mut isolated_envs = envs.to_vec();
    isolated_envs.extend([
        (
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        ),
        ("GIT_CONFIG_NOSYSTEM", "1"),
    ]);
    // An empty input closes stdin instead of inheriting the caller's.
    let output = capture_bytes_in_dir_with_input_env_budget(
        program,
        args,
        (cwd, Some(input.unwrap_or_default())),
        (&isolated_envs, &ambient),
        budget,
        &context,
    )?;
    if output.timed_out {
        return Err(format!("{context}: timed out after {STEP_TIMEOUT:?}"));
    }
    Ok(Captured {
        exit: output.status.and_then(|status| status.code()),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let owned: Vec<String> = [
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.autocrlf=false",
        "-c",
        "core.hooksPath=.ripr-agent-eval-no-hooks",
    ]
    .iter()
    .chain(args.iter())
    .map(|value| value.to_string())
    .collect();
    let output = capture(Path::new("git"), &owned, root, None, &GIT_IDENTITY)?;
    if output.exit == Some(0) {
        Ok(())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            output.stderr.trim()
        ))
    }
}

/// A fresh two-commit repository: `main` at base, `change` checked out at
/// head. Every route gets its own, so first-run state cannot leak between
/// routes.
fn materialize(scratch: &Path, scenario: &Scenario, label: &str) -> Result<Workspace, String> {
    let root = scratch.join(format!("{}-{label}", scenario.id));
    fs::create_dir_all(&root)
        .map_err(|err| format!("failed to create {}: {err}", root.display()))?;
    write_file(
        &root.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
            scenario.crate_name
        ),
    )?;
    write_file(&root.join(".gitignore"), "/target/\n")?;
    write_file(&root.join("src/lib.rs"), scenario.base_lib)?;
    write_file(&root.join("tests/existing.rs"), scenario.existing_tests)?;
    git(&root, &["init", "-q", "-b", "main"])?;
    git(&root, &["add", "-A"])?;
    git(&root, &["commit", "-q", "-m", "base"])?;
    git(&root, &["checkout", "-q", "-b", "change"])?;
    write_file(&root.join("src/lib.rs"), scenario.head_lib)?;
    git(&root, &["commit", "-q", "-am", "change"])?;
    let mut spellings = vec![root.display().to_string(), normalize_path(&root)];
    if let Ok(canonical) = fs::canonicalize(&root) {
        spellings.push(canonical.display().to_string());
        spellings.push(normalize_path(&canonical));
    }
    // Longest first so a canonical spelling that extends another is replaced whole.
    spellings.sort_by_key(|spelling| std::cmp::Reverse(spelling.len()));
    spellings.dedup();
    Ok(Workspace { root, spellings })
}

fn cli_step(
    ripr: &Path,
    workspace: &Workspace,
    label: &str,
    args: &[String],
) -> Result<Step, String> {
    let output = capture(ripr, args, &workspace.root, None, &[])?;
    Ok(raw_step(label, &output, workspace))
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|value| value.to_string()).collect()
}

fn find_args() -> Vec<String> {
    strings(&["check", "--quiet", "--format", "agent", "--base", "main"])
}

/// `next=ripr ...` commands printed by an agent-format report, as argv
/// without the leading `ripr`, read from the raw (unnormalized) output.
fn printed_next_commands(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter_map(|line| line.strip_prefix("next=ripr "))
        .map(split_command)
        .collect()
}

/// Split a printed command into argv, keeping `'...'` and `"..."` quoted
/// arguments (a scratch path with spaces) whole and reading the POSIX `'\''`
/// form of an embedded apostrophe. Other backslashes stay literal, so a
/// Windows path is not mangled.
fn split_command(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current: Option<String> = None;
    let mut quote = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(open), _) if ch == open => quote = None,
            (Some(_), _) => current.get_or_insert_with(String::new).push(ch),
            (None, '\\') if chars.peek() == Some(&'\'') => {
                chars.next();
                current.get_or_insert_with(String::new).push('\'');
            }
            (None, '\'' | '"') => {
                quote = Some(ch);
                current.get_or_insert_with(String::new);
            }
            (None, _) if ch.is_whitespace() => args.extend(current.take()),
            (None, _) => current.get_or_insert_with(String::new).push(ch),
        }
    }
    args.extend(current);
    args
}

/// A baseline must carry every field `compare` guards, or a row missing one
/// would skip that guard and the compare would pass vacuously. Nulls are
/// allowed only where a run legitimately leaves a value unmeasured: a
/// reached answer has a token count, and a scenario that still exists has a
/// loop flag exactly when it defines what that flag measures.
fn validate_baseline(baseline: &Value) -> Result<(), String> {
    let count = |value: &Value| value.is_u64();
    let route_ok = |route: &Value| {
        let reached = route["answer_reached"].as_bool();
        route["route"]
            .as_str()
            .is_some_and(|name| ROUTES.contains(&name))
            && reached.is_some()
            && count(&route["failed_steps"])
            && count(&route["total_tokens"])
            && route.get("tokens_to_answer").is_some_and(|tokens| {
                if reached == Some(true) {
                    count(tokens)
                } else {
                    tokens.is_null()
                }
            })
    };
    let scenario_ok = |row: &Value| {
        let checks = &row["loop"];
        let known = row["id"]
            .as_str()
            .and_then(|id| SCENARIOS.iter().find(|scenario| scenario.id == id));
        // Present, and a bool where the scenario measures it, else null. A
        // scenario the eval no longer defines keeps the type check only;
        // compare reports it as no longer running.
        let flag = |name: &str, measured: Option<bool>| {
            checks.get(name).is_some_and(|value| match measured {
                Some(true) => value.is_boolean(),
                Some(false) => value.is_null(),
                None => value.is_boolean() || value.is_null(),
            })
        };
        let control = known.map(|scenario| scenario.gaps.is_empty());
        let weak = known.map(|scenario| scenario.weak_test.is_some());
        let reference = known.map(|scenario| scenario.reference_test.is_some());
        row["id"].is_string()
            && row["routes"]
                .as_array()
                .is_some_and(|routes| !routes.is_empty() && routes.iter().all(route_ok))
            && count(&checks["missed_gaps"])
            && count(&checks["extra_gaps"])
            && flag("silent_on_control", control)
            && flag("weak_test_still_flagged", weak)
            && flag("weak_key_valid", weak)
            && flag("reference_test_closes_loop", reference)
            && flag("reference_key_valid", reference)
    };
    let valid = baseline["scenarios"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(scenario_ok));
    if baseline["schema_version"] == SCHEMA_VERSION && valid {
        Ok(())
    } else {
        Err(format!(
            "is not a complete {SCHEMA_VERSION} baseline; rerun with --write-baseline"
        ))
    }
}

/// The `init --agents` loop: FIND, then follow each printed `next=` command
/// (explain, then the context it points at), as the block tells an agent to.
fn route_cli_agent_loop(ripr: &Path, workspace: &Workspace) -> Result<Vec<Step>, String> {
    let find = capture(ripr, &find_args(), &workspace.root, None, &[])?;
    let mut steps = vec![raw_step("find", &find, workspace)];
    let mut queue: Vec<(String, Vec<String>)> = printed_next_commands(&find.stdout)
        .into_iter()
        .map(|args| ("detail".to_string(), args))
        .collect();
    while let Some((label, args)) = queue.first().cloned() {
        queue.remove(0);
        if steps.len() >= MAX_FOLLOWED_STEPS {
            break;
        }
        let output = capture(ripr, &args, &workspace.root, None, &[])?;
        let name = args.first().cloned().unwrap_or_default();
        steps.push(raw_step(&format!("{label}:{name}"), &output, workspace));
        for next in printed_next_commands(&output.stdout) {
            queue.push(("detail".to_string(), next));
        }
    }
    Ok(steps)
}

fn raw_step(label: &str, output: &Captured, workspace: &Workspace) -> Step {
    let text = workspace.normalize(&format!("{}{}", output.stdout, output.stderr));
    Step {
        label: label.to_string(),
        exit: output.exit,
        bytes: text.len() as u64,
        tokens: estimate_tokens(&text),
        text,
    }
}

fn mcp_requests(gap_ids: &[String], list_only: bool) -> String {
    let mut lines = vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ripr-agent-eval","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    if !list_only {
        lines.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        lines.push(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ripr_workspace_status","arguments":{}}}));
    }
    lines.push(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ripr_refresh","arguments":{"base":"main"}}}));
    lines.push(json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"ripr_list_gaps","arguments":{}}}));
    for (index, id) in gap_ids.iter().enumerate() {
        lines.push(json!({"jsonrpc":"2.0","id":10 + index,"method":"tools/call","params":{"name":"ripr_get_gap","arguments":{"canonical_id":id}}}));
    }
    let mut out = String::new();
    for line in lines {
        out.push_str(&line.to_string());
        out.push('\n');
    }
    out
}

fn mcp_label(id: i64) -> String {
    match id {
        1 => "initialize".to_string(),
        2 => "tools/list".to_string(),
        3 => "ripr_workspace_status".to_string(),
        4 => "ripr_refresh".to_string(),
        5 => "ripr_list_gaps".to_string(),
        other => format!("ripr_get_gap#{}", other - 9),
    }
}

/// The model-visible part of one MCP response.
fn mcp_visible(id: i64, response: &Value) -> String {
    let result = response.get("result");
    match id {
        1 => result
            .and_then(|value| value.get("instructions"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        2 => result
            .and_then(|value| value.get("tools"))
            .map(Value::to_string)
            .unwrap_or_default(),
        _ => match result
            .and_then(|value| value.get("content"))
            .and_then(Value::as_array)
        {
            Some(content) => content
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
            // An error response is what the model sees instead.
            None => response
                .get("error")
                .map(Value::to_string)
                .unwrap_or_default(),
        },
    }
}

/// Request ids in `input` (one JSON-RPC message per line) with no response.
fn unanswered_ids(input: &str, answered: &[i64]) -> Vec<i64> {
    input
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|request| request.get("id").and_then(Value::as_i64))
        .filter(|id| !answered.contains(id))
        .collect()
}

fn mcp_session(
    ripr: &Path,
    workspace: &Workspace,
    input: &str,
) -> Result<Vec<(i64, String, Value)>, String> {
    let output = capture(
        ripr,
        &strings(&["mcp", "--stdio"]),
        &workspace.root,
        Some(input.as_bytes()),
        // The registration docs/AGENT_QUICKSTART.md recommends.
        &[("RIPR_MCP_TOOLS_LIST", "slim")],
    )?;
    let mut responses = Vec::new();
    for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line)
            .map_err(|err| format!("mcp response is not JSON ({err}): {line}"))?;
        let Some(id) = value.get("id").and_then(Value::as_i64) else {
            continue;
        };
        responses.push((id, line.to_string(), value));
    }
    let answered: Vec<i64> = responses.iter().map(|(id, _, _)| *id).collect();
    match mcp_session_problem(output.exit, &unanswered_ids(input, &answered)) {
        None => Ok(responses),
        Some(problem) => Err(problem),
    }
}

/// A session counts only when the server answered every request and then
/// exited 0. A server that exits early answers fewer requests, and one that
/// fails after answering is not a clean run; measuring either would record a
/// cheap, clean route. Both are instrument failures.
fn mcp_session_problem(exit: Option<i32>, missing: &[i64]) -> Option<String> {
    if !missing.is_empty() {
        Some(format!(
            "ripr mcp --stdio exited {exit:?} without answering request ids {missing:?}"
        ))
    } else if exit != Some(0) {
        Some(format!(
            "ripr mcp --stdio answered every request but exited {exit:?}"
        ))
    } else {
        None
    }
}

fn route_mcp(ripr: &Path, workspace: &Workspace) -> Result<Vec<Step>, String> {
    // A first session lists the gaps, so the measured session can read each
    // one the way an agent would after seeing the list.
    let listing = mcp_session(ripr, workspace, &mcp_requests(&[], true))?;
    let gap_ids: Vec<String> = listing
        .iter()
        .filter(|(id, _, _)| *id == 5)
        .filter_map(|(_, _, value)| value.pointer("/result/structuredContent/items"))
        .filter_map(Value::as_array)
        .flatten()
        // An agent reads the items that are not already exposed.
        .filter(|item| item.get("class").and_then(Value::as_str) != Some("exposed"))
        .filter_map(|item| item.get("canonical_id").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let session = mcp_session(ripr, workspace, &mcp_requests(&gap_ids, false))?;
    Ok(session
        .iter()
        .map(|(id, line, value)| {
            let text = workspace.normalize(&mcp_visible(*id, value));
            Step {
                label: mcp_label(*id),
                // A JSON-RPC error or a tool result flagged `isError` failed.
                exit: Some(i32::from(
                    value.get("error").is_some()
                        || value.pointer("/result/isError") == Some(&Value::Bool(true)),
                )),
                bytes: workspace.normalize(line).len() as u64,
                tokens: estimate_tokens(&text),
                text,
            }
        })
        .collect())
}

fn run_route(ripr: &Path, workspace: &Workspace, route: &str) -> Result<Vec<Step>, String> {
    let base = ["--base", "main"];
    match route {
        "cli-agent-loop" => route_cli_agent_loop(ripr, workspace),
        "cli-agent-find" => Ok(vec![cli_step(ripr, workspace, "find", &find_args())?]),
        "cli-human" => Ok(vec![cli_step(
            ripr,
            workspace,
            "check",
            &strings(&["check", base[0], base[1]]),
        )?]),
        "cli-json-terse" => Ok(vec![cli_step(
            ripr,
            workspace,
            "check",
            &strings(&["check", base[0], base[1], "--format", "json-terse"]),
        )?]),
        "agent-brief" => Ok(vec![cli_step(
            ripr,
            workspace,
            "brief",
            &strings(&["agent", "brief", "--json", base[0], base[1]]),
        )?]),
        "mcp" => route_mcp(ripr, workspace),
        other => Err(format!("unknown route {other}")),
    }
}

/// True when `text` holds `prefix` immediately followed by `number` and then
/// a non-digit, so `lib.rs:5` does not match `lib.rs:50`.
fn has_number_after(text: &str, prefix: &str, number: u32) -> bool {
    let needle = format!("{prefix}{number}");
    text.match_indices(&needle).any(|(index, _)| {
        !text[index + needle.len()..]
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_digit())
    })
}

fn has_line(text: &str, line: u32) -> bool {
    ["lib.rs:", "\"line\":", "\"line\": "]
        .iter()
        .any(|prefix| has_number_after(text, prefix, line))
}

fn gap_visible(text: &str, gap: &GapKey) -> bool {
    text.contains(gap.owner)
        && has_line(text, gap.line)
        && gap
            .facts
            .iter()
            .all(|spellings| spellings.iter().any(|spelling| text.contains(spelling)))
}

/// Steps that carry a route's verdict on the diff. With no keyed gaps the
/// answer is "nothing to do", known once the first of these has been read;
/// whether the route really stayed silent is judged by the loop checks.
const DISCOVERY_STEPS: [&str; 4] = ["find", "check", "brief", "ripr_list_gaps"];

/// Cumulative tokens at the first step after which the answer is visible.
fn tokens_to_answer(steps: &[Step], gaps: &[GapKey]) -> Option<u64> {
    let mut seen = String::new();
    let mut tokens = 0;
    for step in steps {
        if gaps.is_empty() {
            tokens += step.tokens;
            if DISCOVERY_STEPS.contains(&step.label.as_str()) {
                // A failed or empty discovery step is not an answer.
                let usable = step.exit == Some(0) && !step.text.trim().is_empty();
                return usable.then_some(tokens);
            }
            continue;
        }
        seen.push_str(&step.text);
        seen.push('\n');
        tokens += step.tokens;
        if gaps.iter().all(|gap| gap_visible(&seen, gap)) {
            return Some(tokens);
        }
    }
    None
}

/// Changed lines of every gap row the agent report lists
/// (`class|src/lib.rs:N|...`), one entry per row, so two rows on one line
/// count twice.
fn reported_gap_lines(agent_report: &str) -> Vec<u32> {
    let mut lines: Vec<u32> = agent_report
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('|');
            let class = fields.next()?;
            // Gap rows start with a bare class; `verify=`/`stages=` rows and
            // the `#ripr` header do not. The expression may contain `=`.
            if class.contains('=') || class.starts_with('#') {
                return None;
            }
            fields.next()?.rsplit_once(':')?.1.parse().ok()
        })
        .collect();
    lines.sort_unstable();
    lines
}

/// Keyed lines with no gap row, and gap rows beyond one per keyed line.
fn missed_and_extra(expected: &[u32], reported: &[u32]) -> (usize, usize) {
    let mut unmatched = reported.to_vec();
    let mut missed = 0;
    for line in expected {
        match unmatched.iter().position(|row| row == line) {
            Some(index) => {
                unmatched.remove(index);
            }
            None => missed += 1,
        }
    }
    (missed, unmatched.len())
}

fn gaps_header_count(agent_report: &str) -> Option<u64> {
    agent_report
        .lines()
        .find(|line| line.starts_with("#ripr agent"))?
        .split_whitespace()
        .find_map(|field| field.strip_prefix("gaps="))
        .and_then(|value| value.parse().ok())
}

fn verify_complete(agent_report: &str) -> bool {
    agent_report
        .lines()
        .any(|line| line.starts_with("verify=complete"))
}

/// What one `cargo test` run established.
#[derive(Debug, PartialEq)]
enum Suite {
    Pass,
    /// The tests compiled, ran, and at least one failed.
    TestFailed,
    /// Ran but gave no test result (for example a compile error): no
    /// evidence either way.
    NotEstablished(String),
    /// The run itself could not be measured (cargo missing, timeout, a
    /// signal, an output budget).
    InstrumentError(String),
}

/// Classify a finished `cargo test` run. Only a run that compiled and
/// reported a failing test counts as a failure; a revert that stops the
/// crate compiling must not credit the agent's tests.
fn classify_suite(exit: Option<i32>, output: &str) -> Suite {
    let compile_error = output.contains("could not compile") || output.contains("error[E");
    match exit {
        Some(0) if !compile_error => Suite::Pass,
        Some(101) if !compile_error && output.contains("test result: FAILED") => Suite::TestFailed,
        None => Suite::InstrumentError("cargo test ended without an exit code".to_string()),
        Some(code) => {
            Suite::NotEstablished(format!("cargo test exit {code} without a test result"))
        }
    }
}

/// Run one test target. A run that did not compile or report a test result
/// is an instrument failure and comes back as `Err`, so it is never read as
/// the key holding or failing.
fn cargo_test(root: &Path, target_dir: &Path, test: &str) -> Result<bool, String> {
    let target = target_dir.display().to_string();
    let output = capture(
        Path::new("cargo"),
        &strings(&["test", "--offline", "-q", "--test", test]),
        root,
        None,
        &[("CARGO_TARGET_DIR", target.as_str())],
    )?;
    let text = format!("{}\n{}", output.stdout, output.stderr);
    match classify_suite(output.exit, &text) {
        Suite::Pass => Ok(true),
        Suite::TestFailed => Ok(false),
        Suite::NotEstablished(reason) | Suite::InstrumentError(reason) => {
            Err(format!("cargo test --test {test}: {reason}"))
        }
    }
}

/// A cargo key check: pass at head, then at base (with `src/lib.rs` put back
/// to head afterwards whatever happened).
fn cargo_key(
    root: &Path,
    target_dir: &Path,
    scenario: &Scenario,
    test: &str,
) -> Result<(bool, bool), String> {
    let at_head = cargo_test(root, target_dir, test);
    write_file(&root.join("src/lib.rs"), scenario.base_lib)?;
    let at_base = cargo_test(root, target_dir, test);
    write_file(&root.join("src/lib.rs"), scenario.head_lib)?;
    Ok((at_head?, at_base?))
}

/// Store a cargo key result, or null plus the instrument failure.
fn record_key(
    result: &mut Value,
    flag: &str,
    outcome: Result<(bool, bool), String>,
    holds: fn(bool, bool) -> bool,
) {
    match outcome {
        Ok((at_head, at_base)) => result[flag] = json!(holds(at_head, at_base)),
        Err(reason) => {
            result[flag] = Value::Null;
            result["cargo_not_established"] = json!(reason);
        }
    }
}

fn add_intent(root: &Path, relative: &str) -> Result<(), String> {
    git(root, &["add", "-N", relative])
}

/// Loop checks on one dedicated workspace, plus optional cargo key checks.
fn usefulness(
    ripr: &Path,
    scratch: &Path,
    scenario: &Scenario,
    skip_cargo: bool,
) -> Result<Value, String> {
    let workspace = materialize(scratch, scenario, "loop")?;
    let expected: Vec<u32> = scenario.gaps.iter().map(|gap| gap.line).collect();
    let find = capture(ripr, &find_args(), &workspace.root, None, &[])?;
    let reported = reported_gap_lines(&find.stdout);
    let (missed, extra) = missed_and_extra(&expected, &reported);
    let header = gaps_header_count(&find.stdout);
    let mut result = json!({
        "find_exit": find.exit,
        "expected_gap_lines": expected,
        "reported_gap_lines": reported,
        "missed_gaps": missed,
        "extra_gaps": extra,
        "gaps_header": header,
        // Silence counts only from a report that ran and said `gaps=0`.
        "silent_on_control": if scenario.gaps.is_empty() {
            Some(
                find.exit == Some(0)
                    && header == Some(0)
                    && reported.is_empty()
                    && !find.stdout.lines().any(|line| line.starts_with("stop=")),
            )
        } else {
            None
        },
        "weak_test_still_flagged": Value::Null,
        "reference_test_closes_loop": Value::Null,
        "reference_key_valid": Value::Null,
        "weak_key_valid": Value::Null,
    });
    let target_dir = scratch.join("cargo-target");
    if let Some(weak) = scenario.weak_test {
        write_file(&workspace.root.join("tests/eval_weak.rs"), weak)?;
        add_intent(&workspace.root, "tests/eval_weak.rs")?;
        let after = capture(ripr, &find_args(), &workspace.root, None, &[])?;
        let still = reported_gap_lines(&after.stdout);
        result["weak_test_still_flagged"] = json!(
            after.exit == Some(0)
                && expected.iter().all(|line| still.contains(line))
                && !verify_complete(&after.stdout)
        );
        if !skip_cargo {
            let outcome = cargo_key(&workspace.root, &target_dir, scenario, "eval_weak");
            record_key(&mut result, "weak_key_valid", outcome, |head, base| {
                head && base
            });
        }
        git(
            &workspace.root,
            &["rm", "-q", "--cached", "tests/eval_weak.rs"],
        )?;
        fs::remove_file(workspace.root.join("tests/eval_weak.rs"))
            .map_err(|err| format!("failed to remove weak test: {err}"))?;
    }
    if let Some(reference) = scenario.reference_test {
        write_file(&workspace.root.join("tests/eval_reference.rs"), reference)?;
        add_intent(&workspace.root, "tests/eval_reference.rs")?;
        let after = capture(ripr, &find_args(), &workspace.root, None, &[])?;
        let still = reported_gap_lines(&after.stdout);
        result["reference_test_closes_loop"] = json!(
            after.exit == Some(0)
                && verify_complete(&after.stdout)
                && expected.iter().all(|line| !still.contains(line))
        );
        if !skip_cargo {
            let outcome = cargo_key(&workspace.root, &target_dir, scenario, "eval_reference");
            record_key(&mut result, "reference_key_valid", outcome, |head, base| {
                head && !base
            });
        }
    }
    Ok(result)
}

fn route_json(route: &str, steps: &[Step], scenario: &Scenario) -> Value {
    let to_answer = tokens_to_answer(steps, scenario.gaps);
    json!({
        "route": route,
        "steps": steps.iter().map(Step::to_json).collect::<Vec<_>>(),
        "total_bytes": steps.iter().map(|step| step.bytes).sum::<u64>(),
        "total_tokens": steps.iter().map(|step| step.tokens).sum::<u64>(),
        "tokens_to_answer": to_answer,
        "answer_reached": to_answer.is_some(),
        "failed_steps": steps.iter().filter(|step| step.exit != Some(0)).count(),
    })
}

fn ripr_version(ripr: &Path) -> String {
    capture(ripr, &strings(&["--version"]), Path::new("."), None, &[])
        .map(|output| output.stdout.trim().to_string())
        .unwrap_or_else(|error| format!("unknown ({error})"))
}

fn build_report(options: &Options, ripr: &Path) -> Result<Value, String> {
    let scratch = scratch_root()?;
    let mut scenario_rows = Vec::new();
    for scenario in SCENARIOS.iter().filter(|scenario| {
        options.scenarios.is_empty() || options.scenarios.iter().any(|id| id == scenario.id)
    }) {
        let mut routes = Vec::new();
        for route in ROUTES {
            let workspace = materialize(&scratch, scenario, route)?;
            let steps = run_route(ripr, &workspace, route)
                .map_err(|error| format!("{} {route}: {error}", scenario.id))?;
            routes.push(route_json(route, &steps, scenario));
        }
        let loop_checks = usefulness(ripr, &scratch, scenario, options.skip_cargo)
            .map_err(|error| format!("{} loop: {error}", scenario.id))?;
        scenario_rows.push(json!({
            "id": scenario.id,
            "summary": scenario.summary,
            "keyed_gaps": scenario.gaps.len(),
            "routes": routes,
            "loop": loop_checks,
        }));
    }
    // Best effort: a leftover scratch dir is harmless and lives under temp.
    let _ = fs::remove_dir_all(&scratch);
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "advisory": true,
        "ripr_version": ripr_version(ripr),
        "token_estimate": "ceil(len/4) per ASCII word run + 1 per other non-whitespace char; not a tokenizer count",
        "routes": summarize_routes(&scenario_rows),
        "scenarios": scenario_rows,
    }))
}

fn summarize_routes(scenarios: &[Value]) -> Value {
    let rows: Vec<Value> = ROUTES
        .iter()
        .map(|route| {
            let cells: Vec<&Value> = scenarios
                .iter()
                .filter_map(|scenario| scenario["routes"].as_array())
                .flatten()
                .filter(|cell| cell["route"] == *route)
                .collect();
            let answered = cells.iter().filter(|cell| cell["answer_reached"] == true).count();
            json!({
                "route": route,
                "scenarios": cells.len(),
                "answered": answered,
                "total_tokens": cells.iter().filter_map(|cell| cell["total_tokens"].as_u64()).sum::<u64>(),
                "tokens_to_answer": cells.iter().filter_map(|cell| cell["tokens_to_answer"].as_u64()).sum::<u64>(),
            })
        })
        .collect();
    Value::Array(rows)
}

/// Loop checks only the cargo key validation fills in.
const CARGO_FLAGS: [&str; 2] = ["reference_key_valid", "weak_key_valid"];

const LOOP_FLAGS: [&str; 5] = [
    "silent_on_control",
    "weak_test_still_flagged",
    "reference_test_closes_loop",
    "reference_key_valid",
    "weak_key_valid",
];

/// Regressions of `current` against `baseline`: lost answers, token growth
/// past the margin, more failed steps, more missed or extra gaps, and loop
/// flags that were true and no longer are. A row new in `current` is a note.
/// With `full_run`, a baseline scenario or route missing from `current` is a
/// regression; a `--scenario` subset skips that check.
pub(crate) fn compare(
    current: &Value,
    baseline: &Value,
    full_run: bool,
) -> (Vec<String>, Vec<String>) {
    let mut regressions = Vec::new();
    let mut notes = Vec::new();
    let empty = Vec::new();
    let base_scenarios = baseline["scenarios"].as_array().unwrap_or(&empty);
    let current_scenarios = current["scenarios"].as_array().unwrap_or(&empty);
    if full_run {
        for base in base_scenarios {
            let id = base["id"].as_str().unwrap_or_default();
            let Some(now) = current_scenarios.iter().find(|row| row["id"] == id) else {
                regressions.push(format!("{id}: baseline scenario missing from this run"));
                continue;
            };
            for old in base["routes"].as_array().unwrap_or(&empty) {
                let route = old["route"].as_str().unwrap_or_default();
                let present = now["routes"]
                    .as_array()
                    .is_some_and(|routes| routes.iter().any(|row| row["route"] == route));
                if !present {
                    regressions.push(format!(
                        "{id} {route}: baseline route missing from this run"
                    ));
                }
            }
        }
    }
    for scenario in current["scenarios"].as_array().unwrap_or(&empty) {
        let id = scenario["id"].as_str().unwrap_or_default();
        let Some(base) = base_scenarios.iter().find(|row| row["id"] == id) else {
            notes.push(format!("{id}: new scenario, no baseline"));
            continue;
        };
        for cell in scenario["routes"].as_array().unwrap_or(&empty) {
            let route = cell["route"].as_str().unwrap_or_default();
            let Some(old) = base["routes"]
                .as_array()
                .and_then(|routes| routes.iter().find(|row| row["route"] == route))
            else {
                notes.push(format!("{id} {route}: new route, no baseline"));
                continue;
            };
            if old["answer_reached"] == true && cell["answer_reached"] != true {
                regressions.push(format!("{id} {route}: answer no longer reached"));
            }
            let (now_failed, then_failed) = (
                cell["failed_steps"].as_u64().unwrap_or(u64::MAX),
                old["failed_steps"].as_u64().unwrap_or(0),
            );
            if now_failed > then_failed {
                regressions.push(format!(
                    "{id} {route}: failed steps {then_failed} -> {}",
                    cell["failed_steps"]
                ));
            }
            for metric in ["total_tokens", "tokens_to_answer"] {
                let (Some(now), Some(then)) = (cell[metric].as_u64(), old[metric].as_u64()) else {
                    continue;
                };
                let limit = then
                    .saturating_add(then / REGRESSION_DIVISOR)
                    .saturating_add(REGRESSION_SLACK_TOKENS);
                if now > limit {
                    regressions.push(format!("{id} {route}: {metric} {then} -> {now}"));
                } else if now != then {
                    notes.push(format!("{id} {route}: {metric} {then} -> {now}"));
                }
            }
        }
        for count in ["missed_gaps", "extra_gaps"] {
            let (Some(now), Some(then)) = (
                scenario["loop"][count].as_u64(),
                base["loop"][count].as_u64(),
            ) else {
                continue;
            };
            if now > then {
                regressions.push(format!("{id}: {count} {then} -> {now}"));
            } else if now < then {
                notes.push(format!("{id}: {count} {then} -> {now}"));
            }
        }
        // A cargo run that could not build or report is an instrument
        // failure: it fails the compare under its own name, not as a NO.
        if let Some(reason) = scenario["loop"]["cargo_not_established"].as_str() {
            regressions.push(format!("{id}: cargo key check not established: {reason}"));
        }
        for flag in LOOP_FLAGS {
            let (now, then) = (&scenario["loop"][flag], &base["loop"][flag]);
            // `--skip-cargo` leaves the cargo-only key checks unmeasured
            // (null); that is not a regression by itself. A scenario that loses its
            // test still regresses through `reference_test_closes_loop`.
            if now.is_null() && CARGO_FLAGS.contains(&flag) {
                continue;
            }
            if then == true && now != true {
                regressions.push(format!("{id}: {flag} true -> {now}"));
            } else if then == false && now == true {
                notes.push(format!("{id}: {flag} false -> true"));
            }
        }
    }
    (regressions, notes)
}

/// A cargo key flag; an instrument failure reads as `not established`, never
/// as the `-` of an unmeasured `--skip-cargo` run.
fn cargo_flag(checks: &Value, name: &str) -> &'static str {
    if checks[name].is_null() && checks["cargo_not_established"].is_string() {
        "not established"
    } else {
        flag(&checks[name])
    }
}

/// Pretty JSON with every object or array of scalars on one line, so one
/// step or route total is one line of the baseline diff.
fn render_json(value: &Value, depth: usize) -> Result<String, String> {
    let scalar = |item: &Value| !item.is_object() && !item.is_array();
    let inline = match value {
        Value::Object(map) => map.values().all(scalar),
        Value::Array(items) => items.iter().all(scalar),
        _ => true,
    };
    if inline {
        return serde_json::to_string(value)
            .map_err(|err| format!("failed to render report: {err}"));
    }
    let pad = "  ".repeat(depth + 1);
    let mut parts = Vec::new();
    if let Value::Object(map) = value {
        for (key, item) in map {
            let key = serde_json::to_string(key).map_err(|err| err.to_string())?;
            parts.push(format!("{pad}{key}: {}", render_json(item, depth + 1)?));
        }
    } else if let Value::Array(items) = value {
        for item in items {
            parts.push(format!("{pad}{}", render_json(item, depth + 1)?));
        }
    }
    let (open, close) = if value.is_object() {
        ('{', '}')
    } else {
        ('[', ']')
    };
    Ok(format!(
        "{open}\n{}\n{}{close}",
        parts.join(",\n"),
        "  ".repeat(depth)
    ))
}

/// Every scenario whose cargo key checks could not run.
fn instrument_failures(report: &Value) -> Vec<String> {
    report["scenarios"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|scenario| {
            let reason = scenario["loop"]["cargo_not_established"].as_str()?;
            let id = scenario["id"].as_str().unwrap_or_default();
            Some(format!("{id}: cargo key check not established: {reason}"))
        })
        .collect()
}

fn flag(value: &Value) -> &'static str {
    match value.as_bool() {
        Some(true) => "yes",
        Some(false) => "NO",
        None => "-",
    }
}

fn render_markdown(report: &Value, comparison: Option<&(Vec<String>, Vec<String>)>) -> String {
    let empty = Vec::new();
    let mut out = String::from("# ripr agent eval\n\n");
    out.push_str(&format!(
        "ripr: `{}`. Tokens are estimates ({}). Advisory; static results are not runtime outcomes.\n\n",
        report["ripr_version"].as_str().unwrap_or_default(),
        report["token_estimate"].as_str().unwrap_or_default()
    ));
    out.push_str("## Routes (all scenarios)\n\n| route | answered | tokens to answer | total tokens |\n|---|---:|---:|---:|\n");
    for row in report["routes"].as_array().unwrap_or(&empty) {
        out.push_str(&format!(
            "| {} | {}/{} | {} | {} |\n",
            row["route"].as_str().unwrap_or_default(),
            row["answered"],
            row["scenarios"],
            row["tokens_to_answer"],
            row["total_tokens"]
        ));
    }
    out.push_str("\nTokens to answer sums only the scenarios a route answered.\n\n## Per scenario\n\n| scenario | route | to answer | total | steps |\n|---|---|---:|---:|---|\n");
    for scenario in report["scenarios"].as_array().unwrap_or(&empty) {
        for cell in scenario["routes"].as_array().unwrap_or(&empty) {
            let steps: Vec<String> = cell["steps"]
                .as_array()
                .unwrap_or(&empty)
                .iter()
                .map(|step| {
                    format!(
                        "{} {}",
                        step["step"].as_str().unwrap_or_default(),
                        step["tokens"]
                    )
                })
                .collect();
            let to_answer = cell["tokens_to_answer"]
                .as_u64()
                .map_or_else(|| "not reached".to_string(), |value| value.to_string());
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                scenario["id"].as_str().unwrap_or_default(),
                cell["route"].as_str().unwrap_or_default(),
                to_answer,
                cell["total_tokens"],
                steps.join(", ")
            ));
        }
    }
    out.push_str("\n## Loop checks (`check --format agent`)\n\n| scenario | keyed | missed | extra | silent on control | weak still flagged | reference closes | reference key valid | weak key valid |\n|---|---:|---:|---:|---|---|---|---|---|\n");
    for scenario in report["scenarios"].as_array().unwrap_or(&empty) {
        let checks = &scenario["loop"];
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            scenario["id"].as_str().unwrap_or_default(),
            scenario["keyed_gaps"],
            checks["missed_gaps"],
            checks["extra_gaps"],
            flag(&checks["silent_on_control"]),
            flag(&checks["weak_test_still_flagged"]),
            flag(&checks["reference_test_closes_loop"]),
            cargo_flag(checks, "reference_key_valid"),
            cargo_flag(checks, "weak_key_valid"),
        ));
    }
    let failures = instrument_failures(report);
    if !failures.is_empty() {
        out.push('\n');
        for line in &failures {
            out.push_str(&format!("- INSTRUMENT FAILURE {line}\n"));
        }
    }
    if let Some((regressions, notes)) = comparison {
        out.push_str("\n## Against the baseline\n\n");
        if regressions.is_empty() {
            out.push_str("No regressions.\n");
        }
        for line in regressions {
            out.push_str(&format!("- REGRESSION {line}\n"));
        }
        for line in notes {
            out.push_str(&format!("- {line}\n"));
        }
    }
    out
}

pub(crate) fn agent_eval(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_options(args)?;
    let ripr = match &options.ripr {
        Some(path) => path.clone(),
        None => {
            crate::run::run("cargo", &["build", "-p", "ripr", "--bin", "ripr"]).and_then(
                |status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err("cargo build -p ripr failed".to_string())
                    }
                },
            )?;
            crate::ripr_debug_binary()
        }
    };
    let ripr =
        fs::canonicalize(&ripr).map_err(|err| format!("ripr binary {}: {err}", ripr.display()))?;
    let report = build_report(&options, &ripr)?;
    let comparison = match &options.compare {
        Some(path) => {
            let body = fs::read_to_string(path)
                .map_err(|err| format!("failed to read baseline {}: {err}", path.display()))?;
            let baseline: Value = serde_json::from_str(&body)
                .map_err(|err| format!("baseline {} is not JSON: {err}", path.display()))?;
            validate_baseline(&baseline)
                .map_err(|error| format!("baseline {} {error}", path.display()))?;
            Some(compare(&report, &baseline, options.scenarios.is_empty()))
        }
        None => None,
    };
    let report_dir = crate::reports_dir();
    fs::create_dir_all(&report_dir)
        .map_err(|err| format!("failed to create {}: {err}", report_dir.display()))?;
    let pretty = render_json(&report, 0)?;
    let json_path = report_dir.join("agent-eval.json");
    let markdown_path = report_dir.join("agent-eval.md");
    write_file(&json_path, &format!("{pretty}\n"))?;
    write_file(
        &markdown_path,
        &render_markdown(&report, comparison.as_ref()),
    )?;
    // An eval that could not measure fails in every mode, and never
    // becomes a baseline that silently stops guarding the cargo keys.
    let failures = instrument_failures(&report);
    if options.write_baseline && failures.is_empty() {
        write_file(
            &crate::repo_root()?.join(BASELINE_PATH),
            &format!("{pretty}\n"),
        )?;
        println!("agent-eval: wrote baseline {BASELINE_PATH}");
    }
    println!(
        "agent-eval: wrote {} and {}",
        normalize_path(&json_path),
        normalize_path(&markdown_path)
    );
    let mut problems = Vec::new();
    if !failures.is_empty() {
        let baseline_note = if options.write_baseline {
            "; the baseline was not written"
        } else {
            ""
        };
        problems.push(format!(
            "{} instrument failure(s){baseline_note}:\n{}",
            failures.len(),
            failures.join("\n")
        ));
    }
    if let Some((regressions, _)) = &comparison
        && !regressions.is_empty()
    {
        problems.push(format!(
            "{} regression(s) against the baseline:\n{}",
            regressions.len(),
            regressions.join("\n")
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("agent-eval: {}", problems.join("\nagent-eval: ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_counts_word_chunks_and_punctuation() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("a b"), 2);
        assert_eq!(estimate_tokens("{\"a\":1}"), 7);
        assert_eq!(estimate_tokens("é"), 1);
    }

    #[test]
    fn printed_next_commands_reads_only_next_lines() {
        let report = "#ripr agent x\nnext=ripr explain --root /r probe:a\nverify=refused\nnext=ripr context --at probe:a\n";
        assert_eq!(
            printed_next_commands(report),
            vec![
                strings(&["explain", "--root", "/r", "probe:a"]),
                strings(&["context", "--at", "probe:a"]),
            ]
        );
    }

    #[test]
    fn reported_gap_lines_reads_gap_rows_including_expressions_with_equals() {
        let report = "#ripr agent static-advisory base=main findings=2 gaps=2\n\
weakly_exposed|src/lib.rs:5|a >= B|missing=a == B|test=t1|id=p\n\
stages=reach:yes\n\
t1=tests/x.rs:4 name exact_value `assert_eq!(f(1), 0);`\n\
reachable_unrevealed|src/lib.rs:12|x|id=q\n\
weakly_exposed|src/lib.rs:12|x|id=r\n\
verify=refused|outcome=complete_with_findings\n";
        // Two rows on line 12 count twice.
        assert_eq!(reported_gap_lines(report), vec![5, 12, 12]);
        assert_eq!(missed_and_extra(&[5, 12], &[5, 12, 12]), (0, 1));
        assert_eq!(missed_and_extra(&[5, 7], &[5]), (1, 0));
        assert_eq!(missed_and_extra(&[], &[3]), (0, 1));
        assert_eq!(gaps_header_count(report), Some(2));
        assert!(!verify_complete(report));
        assert!(verify_complete("verify=complete|outcome=x\n"));
    }

    #[test]
    fn answer_needs_owner_line_and_every_fact() {
        let gap = GapKey {
            owner: "shipping_cents",
            line: 5,
            facts: &[&["FREE", "5000"], &["equal"]],
        };
        let step = |text: &str| Step {
            label: "s".to_string(),
            exit: Some(0),
            bytes: text.len() as u64,
            tokens: 10,
            text: text.to_string(),
        };
        let one = std::slice::from_ref(&gap);
        let partial = [step("shipping_cents src/lib.rs:5 5000")];
        assert_eq!(tokens_to_answer(&partial, one), None);
        let later = [step("shipping_cents src/lib.rs:5 5000"), step("equal")];
        assert_eq!(tokens_to_answer(&later, one), Some(20));
        let json_line = [step(
            "{\"owner\":\"shipping_cents\",\"line\": 5,\"x\":\"FREE equal\"}",
        )];
        assert_eq!(tokens_to_answer(&json_line, one), Some(10));
        // `lib.rs:5` must not match `lib.rs:50` or `"line":50`.
        let wrong_line = [step("shipping_cents src/lib.rs:50 \"line\":50 5000 equal")];
        assert_eq!(tokens_to_answer(&wrong_line, one), None);
        // A control answer is known once the discovery step is read, and
        // only when that step succeeded with output.
        let mut mcp = [step("init"), step("[]")];
        mcp[1].label = "ripr_list_gaps".to_string();
        assert_eq!(tokens_to_answer(&mcp, &[]), Some(20));
        mcp[1].exit = Some(1);
        assert_eq!(tokens_to_answer(&mcp, &[]), None);
        mcp[1].exit = Some(0);
        mcp[1].text = String::new();
        assert_eq!(tokens_to_answer(&mcp, &[]), None);
    }

    fn report(tokens: u64, reached: bool, closes: bool) -> Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "scenarios": [{
                "id": "s",
                "routes": [{"route": "r", "failed_steps": 0, "total_tokens": tokens, "tokens_to_answer": if reached { Some(tokens) } else { None }, "answer_reached": reached}],
                "loop": {"missed_gaps": 0, "extra_gaps": 0, "reference_test_closes_loop": closes},
            }],
        })
    }

    fn compare_full(current: &Value, baseline: &Value) -> Vec<String> {
        compare(current, baseline, true).0
    }

    #[test]
    fn compare_flags_growth_lost_answers_and_flipped_flags() {
        let baseline = report(1000, true, true);
        assert!(compare_full(&report(1100, true, true), &baseline).is_empty());
        // Both the route total and its tokens-to-answer grew past the margin.
        assert_eq!(compare_full(&report(1200, true, true), &baseline).len(), 2);
        let lost = compare_full(&report(900, false, true), &baseline);
        assert!(lost.iter().any(|line| line.contains("no longer reached")));
        assert_eq!(
            compare_full(&report(1000, true, false), &baseline),
            vec!["s: reference_test_closes_loop true -> false".to_string()]
        );
        let (regressions, notes) = compare(&report(500, true, true), &baseline, true);
        assert!(regressions.is_empty());
        assert!(notes.iter().any(|line| line.contains("1000 -> 500")));
        // A flag that disappears is not a pass.
        let mut dropped = report(1000, true, true);
        dropped["scenarios"][0]["loop"]["reference_test_closes_loop"] = Value::Null;
        assert_eq!(
            compare_full(&dropped, &baseline),
            vec!["s: reference_test_closes_loop true -> null".to_string()]
        );
        // More failed steps regress even when the answer survives.
        let mut failing = report(1000, true, true);
        failing["scenarios"][0]["routes"][0]["failed_steps"] = json!(1);
        let failed = compare_full(&failing, &baseline);
        assert!(
            failed
                .iter()
                .any(|line| line.contains("failed steps 0 -> 1"))
        );
        // A baseline row missing from a full run regresses; a subset skips it.
        let empty = json!({"schema_version": SCHEMA_VERSION, "scenarios": []});
        assert_eq!(
            compare_full(&empty, &baseline),
            vec!["s: baseline scenario missing from this run".to_string()]
        );
        assert!(compare(&empty, &baseline, false).0.is_empty());
        // A `--skip-cargo` run leaves the cargo key checks null.
        let mut keyed = report(1000, true, true);
        keyed["scenarios"][0]["loop"]["reference_key_valid"] = json!(true);
        let mut skipped = keyed.clone();
        skipped["scenarios"][0]["loop"]["reference_key_valid"] = Value::Null;
        assert!(compare_full(&skipped, &keyed).is_empty());
        skipped["scenarios"][0]["loop"]["reference_key_valid"] = json!(false);
        assert_eq!(compare_full(&skipped, &keyed).len(), 1);
        // A cargo run that could not build fails under its own name.
        let mut broken = keyed.clone();
        broken["scenarios"][0]["loop"]["reference_key_valid"] = Value::Null;
        broken["scenarios"][0]["loop"]["cargo_not_established"] = json!("no toolchain");
        assert_eq!(
            compare_full(&broken, &keyed),
            vec!["s: cargo key check not established: no toolchain".to_string()]
        );
        assert_eq!(
            instrument_failures(&broken),
            vec!["s: cargo key check not established: no toolchain".to_string()]
        );
        assert!(instrument_failures(&skipped).is_empty());
        let checks = &broken["scenarios"][0]["loop"];
        assert_eq!(cargo_flag(checks, "reference_key_valid"), "not established");
        let rendered = render_json(&json!({"a": [{"b": 1}, {"c": [2]}], "d": {"e": "f"}}), 0);
        assert_eq!(
            rendered,
            Ok("{\n  \"a\": [\n    {\"b\":1},\n    {\n      \"c\": [2]\n    }\n  ],\n  \"d\": {\"e\":\"f\"}\n}".to_string())
        );
        let reparsed: Value =
            serde_json::from_str(&rendered.unwrap_or_default()).unwrap_or_default();
        assert_eq!(reparsed["a"][1]["c"][0], 2);
        let markdown = render_markdown(&broken, None);
        assert!(markdown.ends_with(
            "\n- INSTRUMENT FAILURE s: cargo key check not established: no toolchain\n"
        ));
        assert_eq!(
            cargo_flag(&json!({"reference_key_valid": null}), "reference_key_valid"),
            "-"
        );
        let mut no_route = report(1000, true, true);
        no_route["scenarios"][0]["routes"] = json!([]);
        assert_eq!(
            compare_full(&no_route, &baseline),
            vec!["s r: baseline route missing from this run".to_string()]
        );
    }

    #[test]
    fn scenario_keys_name_lines_that_changed_and_owners_present_at_head() {
        for scenario in SCENARIOS {
            let base: Vec<&str> = scenario.base_lib.lines().collect();
            let head: Vec<&str> = scenario.head_lib.lines().collect();
            for gap in scenario.gaps {
                let index = gap.line as usize - 1;
                assert!(
                    head[index] != base[index],
                    "{}: line {} did not change",
                    scenario.id,
                    gap.line
                );
                assert!(
                    scenario.head_lib.contains(&format!("fn {}(", gap.owner)),
                    "{}: owner {}",
                    scenario.id,
                    gap.owner
                );
            }
            assert_eq!(
                scenario.reference_test.is_some(),
                !scenario.gaps.is_empty(),
                "{}",
                scenario.id
            );
        }
    }

    #[test]
    fn options_reject_unknown_scenarios_and_partial_baselines() {
        let refused = |args: &[&str]| parse_options(&strings(args)).is_err();
        assert!(refused(&["--scenario", "nope"]));
        assert!(refused(&[
            "--scenario",
            "boundary_threshold",
            "--write-baseline"
        ]));
        assert!(refused(&["--write-baseline", "--skip-cargo"]));
        assert!(refused(&["--write-baseline", "--compare", "b.json"]));
        assert!(validate_baseline(&json!({"schema_version": SCHEMA_VERSION})).is_err());
        let route = json!({"route": "mcp", "answer_reached": true, "failed_steps": 0, "total_tokens": 9, "tokens_to_answer": 9});
        let checks = json!({"missed_gaps": 0, "extra_gaps": 0, "silent_on_control": null, "weak_test_still_flagged": true,
            "reference_test_closes_loop": true, "reference_key_valid": true, "weak_key_valid": null});
        let full = json!({"schema_version": SCHEMA_VERSION, "scenarios": [{"id": "s", "routes": [route], "loop": checks}]});
        assert_eq!(validate_baseline(&full), Ok(()));
        // A row missing a guarded field would skip that guard: refused.
        for (path, field) in [
            ("routes", "total_tokens"),
            ("routes", "answer_reached"),
            ("routes", "tokens_to_answer"),
            ("loop", "missed_gaps"),
            ("loop", "reference_test_closes_loop"),
        ] {
            let mut partial = full.clone();
            let target = if path == "routes" {
                &mut partial["scenarios"][0]["routes"][0]
            } else {
                &mut partial["scenarios"][0]["loop"]
            };
            if let Some(map) = target.as_object_mut() {
                map.remove(field);
            }
            assert!(validate_baseline(&partial).is_err(), "{field}");
        }
        let names_only = json!({"schema_version": SCHEMA_VERSION, "scenarios": [{"id": "s", "routes": [{"route": "cli-agent-loop"}]}]});
        assert!(validate_baseline(&names_only).is_err());
        let mut unknown_route = full.clone();
        unknown_route["scenarios"][0]["routes"][0]["route"] = json!("cli-other");
        assert!(validate_baseline(&unknown_route).is_err());
        let mut string_flag = full.clone();
        string_flag["scenarios"][0]["loop"]["weak_key_valid"] = json!("yes");
        assert!(validate_baseline(&string_flag).is_err());
        let mut no_id = full.clone();
        no_id["scenarios"][0]["id"] = json!(7);
        assert!(validate_baseline(&no_id).is_err());
        // An MCP server that stops early leaves requests unanswered; the
        // notification has no id and needs no answer.
        let requests = mcp_requests(&strings(&["gap-a"]), false);
        assert_eq!(
            unanswered_ids(&requests, &[1, 2, 3, 4, 5, 10]),
            Vec::<i64>::new()
        );
        assert_eq!(unanswered_ids(&requests, &[1, 2]), vec![3, 4, 5, 10]);
        assert_eq!(mcp_session_problem(Some(0), &[]), None);
        assert!(mcp_session_problem(Some(42), &[]).is_some());
        assert!(mcp_session_problem(None, &[]).is_some());
        assert!(mcp_session_problem(Some(0), &[4, 5]).is_some());
        let mut reached_no_tokens = full.clone();
        reached_no_tokens["scenarios"][0]["routes"][0]["tokens_to_answer"] = Value::Null;
        assert!(validate_baseline(&reached_no_tokens).is_err());
        let mut unreached = full.clone();
        unreached["scenarios"][0]["routes"][0]["answer_reached"] = json!(false);
        assert!(validate_baseline(&unreached).is_err());
        unreached["scenarios"][0]["routes"][0]["tokens_to_answer"] = Value::Null;
        assert_eq!(validate_baseline(&unreached), Ok(()));
        let committed: Value =
            serde_json::from_str(include_str!("../../../benchmarks/agent_eval/baseline.json"))
                .unwrap_or_default();
        assert_eq!(validate_baseline(&committed), Ok(()));
        // A null where the scenario defines the check would switch its guard
        // off; a bool where it does not would invent one.
        let position = committed["scenarios"].as_array().and_then(|rows| {
            rows.iter()
                .position(|row| row["id"] == "boundary_threshold")
        });
        assert!(position.is_some());
        let row = position.unwrap_or_default();
        for (flag, value) in [
            ("reference_test_closes_loop", Value::Null),
            ("reference_key_valid", Value::Null),
            ("silent_on_control", json!(true)),
        ] {
            let mut edited = committed.clone();
            edited["scenarios"][row]["loop"][flag] = value;
            assert!(validate_baseline(&edited).is_err(), "{flag}");
        }
        assert_eq!(
            split_command("explain --root '/tmp/a b' --at \"x y\" z"),
            strings(&["explain", "--root", "/tmp/a b", "--at", "x y", "z"])
        );
        assert_eq!(
            split_command("explain --root '/tmp/it'\\''s dir' dir\\sub"),
            strings(&["explain", "--root", "/tmp/it's dir", "dir\\sub"])
        );
        let options = parse_options(&strings(&[
            "--scenario",
            "boundary_threshold",
            "--skip-cargo",
        ]));
        assert!(options.is_ok_and(|options| options.skip_cargo && options.scenarios.len() == 1));
    }

    #[test]
    fn only_a_compiled_failing_run_counts_as_a_failure() {
        assert_eq!(classify_suite(Some(0), "test result: ok."), Suite::Pass);
        assert_eq!(
            classify_suite(Some(101), "test result: FAILED. 0 passed; 1 failed"),
            Suite::TestFailed
        );
        // A revert that breaks compilation exits 101 too; it is no catch.
        let compile = "error[E0308]: mismatched types\nerror: could not compile `shop`";
        assert!(matches!(
            classify_suite(Some(101), compile),
            Suite::NotEstablished(_)
        ));
        assert!(matches!(
            classify_suite(None, ""),
            Suite::InstrumentError(_)
        ));
        assert!(matches!(
            classify_suite(Some(101), ""),
            Suite::NotEstablished(_)
        ));
    }
}

#[cfg(test)]
#[path = "agent_eval/isolation_tests.rs"]
mod isolation_tests;
