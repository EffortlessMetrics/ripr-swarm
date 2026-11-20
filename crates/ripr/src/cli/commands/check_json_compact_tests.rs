//! #13103: exercise the production refusal selector and timeout stash without
//! timing-sensitive sleeps or another error/recovery producer.

use super::{
    CheckInput, CoreError, OutputFormat, TimeoutRefusalSite, machine_refusal_document,
    select_output_format, stash_timeout_refusal,
};
use crate::analysis::{InputFactsSlot, InputSource};
use crate::app::timeout_refusal;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct ClearStash;

impl Drop for ClearStash {
    fn drop(&mut self) {
        timeout_refusal::clear_stashed();
    }
}

fn document(input: &CheckInput, format: OutputFormat, error: &CoreError) -> Result<String, String> {
    machine_refusal_document(input, format, error)
        .ok_or_else(|| format!("{format:?} must supply a structured refusal"))?
}

fn parse(document: &str) -> Result<Value, String> {
    serde_json::from_str(document).map_err(|error| format!("invalid refusal JSON: {error}"))
}

fn same_refusal(pretty: &str, compact: &str) -> Result<Value, String> {
    let expected = parse(pretty)?;
    let actual = parse(compact)?;
    if expected != actual {
        return Err("compact presentation changed refusal evidence or recovery".to_string());
    }
    if !compact.ends_with('\n') || compact.lines().count() != 1 || compact.len() >= pretty.len() {
        return Err(
            "refusal must become a smaller single JSON line with a final newline".to_string(),
        );
    }
    if actual["analysis_scope"]["downstream_consumable"] != false {
        return Err("refusal must remain non-consumable, never an all-clear".to_string());
    }
    Ok(actual)
}

#[test]
fn json_compact_typed_refusal_keeps_identity_and_diagnostic_bytes() -> Result<(), String> {
    let input = CheckInput {
        root: PathBuf::from("workspace with spaces"),
        base: Some("review-base".to_string()),
        ..CheckInput::default()
    };
    let error =
        CoreError::repository_root_unusable("missing \"workspace\"; café 雪 \\ path\nnext line");
    let pretty = document(&input, OutputFormat::Json, &error)?;
    let compact = document(&input, OutputFormat::JsonCompact, &error)?;
    let value = same_refusal(&pretty, &compact)?;
    if value["analysis_scope"]["run_status"] != "repository_root_unusable"
        || value["base"] != "review-base"
        || value["findings"].as_array().map(Vec::len) != Some(0)
    {
        return Err(format!(
            "typed refusal lost its identity or context: {value}"
        ));
    }
    Ok(())
}

#[test]
fn json_compact_scope_guards_keep_the_frozen_envelope() -> Result<(), String> {
    let input = CheckInput::default();
    for (detail, status, scope) in [
        (
            "diff_scope_oversized: too many changed lines",
            "diff_scope_oversized",
            "diff",
        ),
        (
            "repo_scope_oversized: too many source files",
            "repo_scope_oversized",
            "repo",
        ),
    ] {
        let error = CoreError::from(detail.to_string());
        let pretty = document(&input, OutputFormat::Json, &error)?;
        let compact = document(&input, OutputFormat::JsonCompact, &error)?;
        let value = same_refusal(&pretty, &compact)?;
        if value["analysis_scope"]["run_status"] != status
            || value["analysis_scope"]["scope"] != scope
        {
            return Err(format!("scope guard changed identity: {value}"));
        }
        let legacy =
            crate::output::limited_check::render_diff_scope_limited_check_json(&input, detail)?
                .ok_or("scope fixture must exercise the scope-guard producer")?;
        if pretty != legacy {
            return Err("the existing JSON scope-guard bytes changed".to_string());
        }
    }
    Ok(())
}

fn timeout_document(
    format: OutputFormat,
    explicit: Option<&str>,
    diff_file: Option<&str>,
) -> Result<String, String> {
    timeout_refusal::clear_stashed();
    let _clear = ClearStash;
    let input = CheckInput {
        root: PathBuf::from("workspace"),
        base: Some("review-base".to_string()),
        diff_file: diff_file.map(PathBuf::from),
        format,
        ..CheckInput::default()
    };
    let error = CoreError::analysis_timeout("analysis_timeout: the analysis budget expired");
    let slot: InputFactsSlot = Arc::new(Mutex::new(None));
    stash_timeout_refusal(&TimeoutRefusalSite {
        input: &input,
        error: &error,
        budget: Some(Duration::from_secs(4)),
        format,
        format_explicit: explicit.map(str::to_string),
        output_dir: None,
        source: if diff_file.is_some() {
            InputSource::SuppliedDiff
        } else {
            InputSource::WorkingTree
        },
        worktree_explicit: diff_file.is_none(),
        committed_explicit: false,
        candidate_tree: None,
        candidate_base: None,
        slot: &slot,
    });
    let rendered = document(&input, format, &error)?;
    if timeout_refusal::take_stashed().is_some() {
        return Err("the refusal must consume its timeout context exactly once".to_string());
    }
    Ok(rendered)
}

#[test]
fn json_compact_timeout_replays_the_explicit_format_and_retains_unknown_facts() -> Result<(), String>
{
    let pretty = timeout_document(OutputFormat::Json, Some("json"), None)?;
    let compact = timeout_document(OutputFormat::JsonCompact, Some("json-compact"), None)?;
    let expected = parse(&pretty)?;
    let mut actual = parse(&compact)?;
    let recovery = &actual["input_context"]["recovery"];
    let command = recovery["command"]
        .as_str()
        .ok_or("missing bounded retry")?;
    let pretty_command = expected["input_context"]["recovery"]["command"]
        .as_str()
        .ok_or("missing legacy bounded retry")?;
    if !command.contains("--format json-compact")
        || command.contains("--json")
        || !command.contains("--worktree")
        || !command.ends_with("--timeout 8")
        || recovery["budget_seconds"] != 8
        || recovery["kind"] != "retry_with_larger_budget"
    {
        return Err(format!(
            "compact timeout changed the requested retry: {recovery}"
        ));
    }
    if command.replace("--format json-compact", "--format json") != pretty_command {
        return Err("only the selected output spelling may differ in the retry".to_string());
    }
    if actual["analysis_scope"]["run_status"] != "analysis_timeout"
        || actual["analysis_scope"]["downstream_consumable"] != false
        || actual["schema_version"] != crate::app::CHECK_REFUSAL_SCHEMA_VERSION
    {
        return Err("timeout must remain the amended, non-consumable refusal".to_string());
    }
    let resolved = actual["input_context"]["resolved"]
        .as_object()
        .ok_or("timeout must retain availability-qualified facts")?;
    if resolved.is_empty()
        || resolved.values().any(|fact| {
            fact["available"] != false || fact["reason"] != "not_published_before_deadline"
        })
    {
        return Err("an early timeout must not invent resolved input facts".to_string());
    }
    // Separate invocations deliberately carry their own retry format. Assert
    // that one difference before comparing every other field unchanged.
    actual["input_context"]["recovery"]["command"] = Value::String(pretty_command.to_string());
    if actual != expected || compact.lines().count() != 1 || !compact.ends_with('\n') {
        return Err("compact timeout changed fields beyond the explicit retry format".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_stdin_timeout_never_offers_a_replay_of_consumed_input() -> Result<(), String> {
    let pretty = timeout_document(OutputFormat::Json, Some("json"), Some("-"))?;
    let compact = timeout_document(OutputFormat::JsonCompact, Some("json-compact"), Some("-"))?;
    let value = same_refusal(&pretty, &compact)?;
    let recovery = &value["input_context"]["recovery"];
    if recovery["kind"] != "save_stdin_diff_to_named_file" || !recovery["command"].is_null() {
        return Err("consumed stdin must not gain an executable retry".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_and_json_are_distinct_but_repeated_compact_selection_is_valid() -> Result<(), String>
{
    let mut selection = None;
    let mut format = OutputFormat::Human;
    for _ in 0..2 {
        select_output_format(
            &mut selection,
            &mut format,
            "--format json-compact".to_string(),
            OutputFormat::JsonCompact,
        )?;
    }
    let error = select_output_format(
        &mut selection,
        &mut format,
        "--json".to_string(),
        OutputFormat::Json,
    )
    .err()
    .ok_or("conflicting output selections must be refused")?;
    if format != OutputFormat::JsonCompact
        || !error.contains("`--format json-compact` and `--json`")
    {
        return Err("a conflict must preserve and name the original selection".to_string());
    }
    Ok(())
}
