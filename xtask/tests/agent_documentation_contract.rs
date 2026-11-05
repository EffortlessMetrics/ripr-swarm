//! Consumer-agent examples must remain portable, discoverable and discriminating.
//! These source-checkout tests do not claim a live RIPR repair or agent compliance.

#[path = "../../crates/ripr/tests/common/help_contract.rs"]
pub mod help_contract;
#[path = "common/ripr_help.rs"]
pub mod ripr_help;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use help_contract::{assert_contains, normalized, undocumented_doc_flags};
use ripr_help::{Scratch, require_success, run, run_ripr};

const DISPATCH: &str = include_str!("../../docs/AGENT_DISPATCH_WORKFLOW.md");
const OPERATOR: &str = include_str!("../../docs/LLM_OPERATOR_GUIDE.md");
const INDEX: &str = include_str!("../../docs/README.md");
const DOCS: &[(&str, &str)] = &[
    ("docs/AGENT_DISPATCH_WORKFLOW.md", DISPATCH),
    ("docs/LLM_OPERATOR_GUIDE.md", OPERATOR),
];
const ROUTES: &[(&str, &str, &str)] = &[
    ("check", "bash", "ripr check --root . --quiet"),
    ("card", "text", "ripr agent card --root . --seam-id ID"),
    ("start", "text", "ripr repair ITEM --root ."),
    (
        "continue",
        "text",
        "ripr continue --root . --attempt ATTEMPT_ID",
    ),
    (
        "status",
        "text",
        "ripr status --root . --attempt ATTEMPT_ID",
    ),
];

fn marked_block(doc: &str, kind: &str, id: &str, language: &str) -> Result<String, String> {
    // Source checkouts can use CRLF; example identity is independent of it.
    let doc = doc.replace("\r\n", "\n");
    let marker = format!("<!-- agent-doc-{kind}: {id} -->");
    if doc.matches(&marker).count() != 1 {
        return Err(format!("expected exactly one example marker `{marker}`"));
    }
    let (_, after) = doc
        .split_once(&marker)
        .ok_or_else(|| format!("missing `{marker}`"))?;
    let body = after
        .strip_prefix(&format!("\n```{language}\n"))
        .ok_or_else(|| format!("`{marker}` must immediately precede a {language} fence"))?;
    let (code, _) = body
        .split_once("\n```")
        .ok_or_else(|| format!("unterminated example `{marker}`"))?;
    Ok(code.to_string())
}

fn check_routes(doc: &str) -> Result<(), String> {
    for (id, language, command) in ROUTES {
        let actual = marked_block(doc, "command", id, language)?;
        if normalized(&actual) != *command {
            return Err(format!("dispatch route `{id}` changed: `{actual}`"));
        }
    }
    Ok(())
}

#[test]
fn dispatch_examples_keep_the_installed_compact_attempt_routes() -> Result<(), String> {
    check_routes(DISPATCH)?;
    check_routes(&DISPATCH.replace("\r\n", "\n").replace('\n', "\r\n"))
}

#[test]
fn dispatch_route_guard_rejects_removed_examples_and_wrong_routes() -> Result<(), String> {
    for (id, _, command) in ROUTES {
        let marker = format!("<!-- agent-doc-command: {id} -->");
        let removed = DISPATCH.replace(&marker, "");
        if check_routes(&removed).is_ok() {
            return Err(format!("removing the `{id}` example still passed"));
        }
        // The original command remains elsewhere in prose: it cannot rescue
        // a wrong command inside the example an agent actually copies.
        let changed = format!(
            "{}\nOriginal command: `{command}`\n",
            DISPATCH.replace(command, "ripr agent packet --root . --seam-id ID --json")
        );
        if check_routes(&changed).is_ok() {
            return Err(format!(
                "wrong `{id}` example was rescued by a prose mention"
            ));
        }
    }
    Ok(())
}

#[test]
fn agent_guides_match_candidate_help_and_reject_wrong_flags() -> Result<(), String> {
    let mut cache = BTreeMap::new();
    let drift = undocumented_doc_flags(DOCS, &mut cache, &run_ripr)?;
    if !drift.is_empty() {
        return Err(format!("agent guide flags drifted:\n{}", drift.join("\n")));
    }
    if cache.values().filter(|help| help.is_some()).count() < 8 {
        return Err("agent guide flag audit lost its command subjects".to_string());
    }
    // Keep the counterexamples in this process: nextest must not pay another
    // cold candidate build just to challenge the same cached help screens.
    for (name, doc) in DOCS {
        for wrong in [
            "ripr check --agent-doc-invalid",
            "ripr continue --attempt ATTEMPT_ID --store store",
            "ripr status --attempt ATTEMPT_ID --store store",
        ] {
            let changed = format!("{doc}\n```text\n{wrong}\n```\n");
            let drift = undocumented_doc_flags(&[(name, &changed)], &mut cache, &run_ripr)?;
            if drift.is_empty() {
                return Err(format!("{name} accepted the wrong example `{wrong}`"));
            }
        }
    }
    Ok(())
}

fn consumer_route(doc: &str) -> Result<&str, String> {
    let rows = doc
        .lines()
        .filter(|line| line.starts_with("| Hand work to a coding agent |"))
        .collect::<Vec<_>>();
    if rows.len() != 1 {
        return Err("index must have one consumer-agent handoff row".to_string());
    }
    rows[0]
        .split('|')
        .nth(2)
        .ok_or_else(|| "handoff row has no destination cell".to_string())
}

fn check_index(doc: &str) -> Result<(), String> {
    let cell = consumer_route(doc)?;
    assert_contains("consumer handoff", cell, "](AGENT_DISPATCH_WORKFLOW.md)")?;
    if cell.contains("](AGENT_WORKFLOWS.md)") {
        return Err("consumer handoff routes through contributor operation".to_string());
    }
    Ok(())
}

#[test]
fn index_routes_consumers_to_the_playbook_not_contributor_operation() -> Result<(), String> {
    check_index(INDEX)?;
    let wrong = INDEX.replace(
        "](AGENT_DISPATCH_WORKFLOW.md)",
        "](AGENT_WORKFLOWS.md) AGENT_DISPATCH_WORKFLOW.md",
    );
    if check_index(&wrong).is_ok() {
        return Err("index guard credited a filename instead of its link destination".to_string());
    }
    Ok(())
}

fn compile_example(root: &Path, name: &str, source: &str) -> Result<PathBuf, String> {
    let path = root.join(format!("{name}.rs"));
    let executable = root.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&path, source).map_err(|error| error.to_string())?;
    let mut command = Command::new("rustc");
    command
        .current_dir(ripr_help::workspace())
        .env("RUSTUP_AUTO_INSTALL", "0")
        .args(["--edition=2024", "--test"])
        .arg(&path)
        .arg("-o")
        .arg(&executable);
    require_success(run(command)?, &format!("compile documented {name} example"))?;
    Ok(executable)
}

fn execute_example(executable: &Path, filter: &str) -> Result<Output, String> {
    let mut command = Command::new(executable);
    command.args(["--exact", filter, "--test-threads=1"]);
    run(command)
}

fn require_one_pass(output: &Output) -> Result<(), String> {
    let text = String::from_utf8_lossy(&output.stdout);
    if !output.status.success()
        || !text.contains("test threshold_contract ... ok")
        || !text
            .contains("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out")
    {
        return Err(format!(
            "the documented test did not execute and pass ({})\n{text}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[test]
fn documented_boundary_oracle_rejects_wrong_behavior_and_zero_subjects() -> Result<(), String> {
    let source = marked_block(DISPATCH, "example", "threshold-oracle", "rust")?;
    let windows_source = marked_block(
        &DISPATCH.replace("\r\n", "\n").replace('\n', "\r\n"),
        "example",
        "threshold-oracle",
        "rust",
    )?;
    if source != windows_source {
        return Err("CRLF changed the documented oracle source".to_string());
    }
    let comparison = "total >= 100";
    let equality = "    assert_eq!(shipping_fee(100), 0);\n";
    if source.matches(comparison).count() != 1 || source.matches(equality).count() != 1 {
        return Err(
            "boundary control lost its unique comparison or equality assertion".to_string(),
        );
    }
    let scratch = Scratch::new("agent-doc-oracle")?;
    let correct = compile_example(&scratch.0, "correct", &source)?;
    require_one_pass(&execute_example(&correct, "threshold_contract")?)?;

    // The negative implementation must compile. Its behavioral assertion,
    // rather than an invalid fixture or missing dependency, must fail.
    let wrong_source = source.replace(comparison, "total > 100");
    let wrong = compile_example(&scratch.0, "wrong", &wrong_source)?;
    let output = execute_example(&wrong, "threshold_contract")?;
    let text = String::from_utf8_lossy(&output.stdout);
    if output.status.success()
        || !text.contains("test threshold_contract ... FAILED")
        || !text.contains("0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out")
        || !text.contains("left: 10")
        || !text.contains("right: 0")
    {
        return Err(format!(
            "wrong comparison did not fail the intended equality assertion ({})\n{text}",
            output.status
        ));
    }

    // Removing only the equality observation makes the wrong implementation
    // pass: below/above examples alone are not the discriminator.
    let weak_source = wrong_source.replace(equality, "");
    let weak = compile_example(&scratch.0, "weak", &weak_source)?;
    require_one_pass(&execute_example(&weak, "threshold_contract")?)?;

    let empty = execute_example(&correct, "missing_test")?;
    if !empty.status.success() || require_one_pass(&empty).is_ok() {
        return Err(
            "zero-test control did not distinguish process success from execution".to_string(),
        );
    }
    Ok(())
}
