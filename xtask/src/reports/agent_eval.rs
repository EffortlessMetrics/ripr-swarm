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
/// A route regresses when its token total grows by more than this fraction
/// plus `REGRESSION_SLACK_TOKENS` over the baseline.
const REGRESSION_FRACTION: f64 = 0.10;
const REGRESSION_SLACK_TOKENS: u64 = 20;
const STEP_TIMEOUT: Duration = Duration::from_secs(300);
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
    let output = capture_bytes_in_dir_with_input_env_budget(
        program,
        args,
        (cwd, input),
        envs,
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
    let owned: Vec<String> = ["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"]
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
    let text = workspace.normalize(&format!("{}{}", output.stdout, output.stderr));
    Ok(Step {
        label: label.to_string(),
        exit: output.exit,
        bytes: text.len() as u64,
        tokens: estimate_tokens(&text),
        text,
    })
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
        .map(|rest| rest.split_whitespace().map(str::to_string).collect())
        .collect()
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
    Ok(responses)
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
                exit: Some(i32::from(value.get("error").is_some())),
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

fn has_line(text: &str, line: u32) -> bool {
    text.contains(&format!("lib.rs:{line}"))
        || text.contains(&format!("\"line\":{line}"))
        || text.contains(&format!("\"line\": {line}"))
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
                return Some(tokens);
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

/// Changed lines the agent report lists as gaps (`class|src/lib.rs:N|...`).
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
    lines.dedup();
    lines
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

fn cargo_test(root: &Path, target_dir: &Path, test: &str) -> Result<bool, String> {
    let target = target_dir.display().to_string();
    let output = capture(
        Path::new("cargo"),
        &strings(&["test", "--offline", "-q", "--test", test]),
        root,
        None,
        &[("CARGO_TARGET_DIR", target.as_str())],
    )?;
    Ok(output.exit == Some(0))
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
    let missed: Vec<u32> = expected
        .iter()
        .copied()
        .filter(|line| !reported.contains(line))
        .collect();
    let extra: Vec<u32> = reported
        .iter()
        .copied()
        .filter(|line| !expected.contains(line))
        .collect();
    let mut result = json!({
        "find_exit": find.exit,
        "expected_gap_lines": expected,
        "reported_gap_lines": reported,
        "missed_gaps": missed.len(),
        "extra_gaps": extra.len(),
        "gaps_header": gaps_header_count(&find.stdout),
        "silent_on_control": if scenario.gaps.is_empty() { Some(reported.is_empty()) } else { None },
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
            expected.iter().all(|line| still.contains(line)) && !verify_complete(&after.stdout)
        );
        if !skip_cargo {
            let at_head = cargo_test(&workspace.root, &target_dir, "eval_weak")?;
            write_file(&workspace.root.join("src/lib.rs"), scenario.base_lib)?;
            let at_base = cargo_test(&workspace.root, &target_dir, "eval_weak")?;
            write_file(&workspace.root.join("src/lib.rs"), scenario.head_lib)?;
            result["weak_key_valid"] = json!(at_head && at_base);
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
            verify_complete(&after.stdout) && expected.iter().all(|line| !still.contains(line))
        );
        if !skip_cargo {
            let at_head = cargo_test(&workspace.root, &target_dir, "eval_reference")?;
            write_file(&workspace.root.join("src/lib.rs"), scenario.base_lib)?;
            let at_base = cargo_test(&workspace.root, &target_dir, "eval_reference")?;
            write_file(&workspace.root.join("src/lib.rs"), scenario.head_lib)?;
            result["reference_key_valid"] = json!(at_head && !at_base);
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

const LOOP_FLAGS: [&str; 5] = [
    "silent_on_control",
    "weak_test_still_flagged",
    "reference_test_closes_loop",
    "reference_key_valid",
    "weak_key_valid",
];

/// Regressions of `current` against `baseline`: lost answers, token growth
/// past the margin, more missed or extra gaps, and loop flags that went from
/// true to false. Rows missing from either side are reported, not guessed.
pub(crate) fn compare(current: &Value, baseline: &Value) -> (Vec<String>, Vec<String>) {
    let mut regressions = Vec::new();
    let mut notes = Vec::new();
    let empty = Vec::new();
    let base_scenarios = baseline["scenarios"].as_array().unwrap_or(&empty);
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
            for metric in ["total_tokens", "tokens_to_answer"] {
                let (Some(now), Some(then)) = (cell[metric].as_u64(), old[metric].as_u64()) else {
                    continue;
                };
                let limit =
                    (then as f64 * (1.0 + REGRESSION_FRACTION)) as u64 + REGRESSION_SLACK_TOKENS;
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
        for flag in LOOP_FLAGS {
            let (now, then) = (&scenario["loop"][flag], &base["loop"][flag]);
            if then == true && now == false {
                regressions.push(format!("{id}: {flag} true -> false"));
            } else if then == false && now == true {
                notes.push(format!("{id}: {flag} false -> true"));
            }
        }
    }
    (regressions, notes)
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
            flag(&checks["reference_key_valid"]),
            flag(&checks["weak_key_valid"]),
        ));
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
            if baseline["schema_version"] != SCHEMA_VERSION {
                return Err(format!(
                    "baseline {} is not {SCHEMA_VERSION}; rerun with --write-baseline",
                    path.display()
                ));
            }
            Some(compare(&report, &baseline))
        }
        None => None,
    };
    let report_dir = crate::reports_dir();
    fs::create_dir_all(&report_dir)
        .map_err(|err| format!("failed to create {}: {err}", report_dir.display()))?;
    let pretty = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to render report: {err}"))?;
    let json_path = report_dir.join("agent-eval.json");
    let markdown_path = report_dir.join("agent-eval.md");
    write_file(&json_path, &format!("{pretty}\n"))?;
    write_file(
        &markdown_path,
        &render_markdown(&report, comparison.as_ref()),
    )?;
    if options.write_baseline {
        write_file(Path::new(BASELINE_PATH), &format!("{pretty}\n"))?;
        println!("agent-eval: wrote baseline {BASELINE_PATH}");
    }
    println!(
        "agent-eval: wrote {} and {}",
        normalize_path(&json_path),
        normalize_path(&markdown_path)
    );
    match comparison {
        Some((regressions, _)) if !regressions.is_empty() => Err(format!(
            "agent-eval: {} regression(s) against the baseline:\n{}",
            regressions.len(),
            regressions.join("\n")
        )),
        _ => Ok(()),
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
verify=refused|outcome=complete_with_findings\n";
        assert_eq!(reported_gap_lines(report), vec![5, 12]);
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
        let partial = [step("shipping_cents src/lib.rs:5 5000")];
        assert_eq!(tokens_to_answer(&partial, std::slice::from_ref(&gap)), None);
        let later = [step("shipping_cents src/lib.rs:5 5000"), step("equal")];
        assert_eq!(
            tokens_to_answer(&later, std::slice::from_ref(&gap)),
            Some(20)
        );
        let json_line = [step(
            "{\"owner\":\"shipping_cents\",\"line\": 5,\"x\":\"FREE equal\"}",
        )];
        assert_eq!(
            tokens_to_answer(&json_line, std::slice::from_ref(&gap)),
            Some(10)
        );
        let wrong_line = [step("shipping_cents src/lib.rs:50 5000 equal")];
        // `lib.rs:5` is a prefix of `lib.rs:50`; the owner and facts still
        // have to match, so this documents the known looseness.
        assert_eq!(
            tokens_to_answer(&wrong_line, std::slice::from_ref(&gap)),
            Some(10)
        );
        // A control answer is known once the discovery step is read.
        let mut mcp = [step("init"), step("[]")];
        mcp[1].label = "ripr_list_gaps".to_string();
        assert_eq!(tokens_to_answer(&mcp, &[]), Some(20));
    }

    fn report(tokens: u64, reached: bool, closes: bool) -> Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "scenarios": [{
                "id": "s",
                "routes": [{"route": "r", "total_tokens": tokens, "tokens_to_answer": if reached { Some(tokens) } else { None }, "answer_reached": reached}],
                "loop": {"missed_gaps": 0, "extra_gaps": 0, "reference_test_closes_loop": closes},
            }],
        })
    }

    #[test]
    fn compare_flags_growth_lost_answers_and_flipped_flags() {
        let baseline = report(1000, true, true);
        let (regressions, _) = compare(&report(1100, true, true), &baseline);
        assert!(regressions.is_empty(), "{regressions:?}");
        // Both the route total and its tokens-to-answer grew past the margin.
        let (regressions, _) = compare(&report(1200, true, true), &baseline);
        assert_eq!(regressions.len(), 2, "{regressions:?}");
        let (regressions, _) = compare(&report(900, false, true), &baseline);
        assert!(
            regressions
                .iter()
                .any(|line| line.contains("no longer reached"))
        );
        let (regressions, _) = compare(&report(1000, true, false), &baseline);
        assert_eq!(
            regressions,
            vec!["s: reference_test_closes_loop true -> false".to_string()]
        );
        let (regressions, notes) = compare(&report(500, true, true), &baseline);
        assert!(regressions.is_empty());
        assert!(notes.iter().any(|line| line.contains("1000 -> 500")));
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
        assert!(parse_options(&strings(&["--scenario", "nope"])).is_err());
        assert!(
            parse_options(&strings(&[
                "--scenario",
                "boundary_threshold",
                "--write-baseline"
            ]))
            .is_err()
        );
        let options = parse_options(&strings(&[
            "--scenario",
            "boundary_threshold",
            "--skip-cargo",
        ]));
        assert!(options.is_ok_and(|options| options.skip_cargo && options.scenarios.len() == 1));
    }
}
