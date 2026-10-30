//! The job summary's first block: what this pull request changed for the
//! reader (#11217).
//!
//! The summary used to open with ripr's own orientation and policy
//! machinery; the pull-request verdict sat near the bottom. This block says
//! it first, from the same `comments.json` the annotations and the PR
//! comment read, in the PR comment's words. It reads only; an absent or
//! unparsable report prints nothing, so the older sections still tell that
//! story.

use crate::output::markdown::code_span;
use crate::output::pr_summary_comment::is_plain_command;
use serde_json::Value;

const MAX_CELL_CHARS: usize = 160;

/// The block's lines, or none when there is no review-comments report.
pub(super) fn headline_lines(report: &Value) -> Vec<String> {
    if !report.is_object() {
        return Vec::new();
    }
    // A report without a `comments` array cannot show a clean result.
    let rows = report.get("comments").and_then(Value::as_array);
    let listed_rows = rows.map_or(0, |rows| rows.len() as u64);
    let stated_rows = report["summary"]["comments"].as_u64().unwrap_or(0);
    // The producer states summary-only items as a count and as an array;
    // trust the larger so a missing count is never a false clean.
    let counted = report["summary"]["summary_only"].as_u64().unwrap_or(0);
    let listed = report
        .get("summary_only")
        .and_then(Value::as_array)
        .map_or(0, |items| items.len() as u64);
    let total = listed_rows.max(stated_rows) + counted.max(listed);
    let mut lines = vec!["### What RIPR found in this pull request".to_string()];
    if total == 0 {
        let complete = report["analysis_outcome"]["analysis_complete"].as_bool() == Some(true);
        if complete && rows.is_some() {
            lines.push("No changed-behavior spot with weak test evidence (static and advisory; not a statement that the tests are sufficient).".to_string());
        } else {
            lines.push("The analysis did not complete or its report is unreadable for this revision, so this is not a clean result. The sections below say what is missing.".to_string());
        }
        return lines;
    }
    let spots = if total == 1 {
        "1 changed-behavior spot where the tests may not notice a wrong result.".to_string()
    } else {
        format!("{total} changed-behavior spots where the tests may not notice a wrong result.")
    };
    lines.push(format!("**{spots}**"));
    // The producer writes both a `summary.summary_only` count and a root
    // `summary_only` array of cards shaped like `comments` rows; a report
    // with only summary-only items still has a first spot to name.
    let first = rows.and_then(|rows| rows.first()).or_else(|| {
        report
            .get("summary_only")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
    });
    if let Some(row) = first {
        let place = format!(
            "{}:{}",
            text(&row["seam"]["file"]),
            row["seam"]["line"].as_u64().unwrap_or(0)
        );
        let owner = text(&row["owner"]);
        lines.push(format!(
            "- First: {} in {}",
            code_span(&place),
            code_span(&owner)
        ));
        if let Some(missing) = row["missing_discriminator"].as_str() {
            lines.push(format!("- Missing check: {}", code_span(&clean(missing))));
        }
        // Shown whole or not at all: a cut command would run something else.
        if let Some(command) = row["llm_guidance"]["repair_command"]
            .as_str()
            .filter(|command| command.starts_with("ripr agent repair "))
            .filter(|command| is_plain_command(command))
        {
            lines.push(format!("- Next step: {}", code_span(command)));
        }
    } else {
        lines.push("- The items are in the PR guidance section below.".to_string());
    }
    lines
}

fn text(value: &Value) -> String {
    clean(value.as_str().unwrap_or("unknown"))
}

/// One line, bounded: the text comes from the analyzed pull request.
fn clean(raw: &str) -> String {
    raw.chars()
        .take(MAX_CELL_CHARS)
        .filter(|ch| !matches!(ch, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}
