//! The one pull-request conversation comment of the generated workflow
//! (`ripr pr-comments summary`).
//!
//! The job summary lives on the Actions run page and the annotations on the
//! Files tab; a reviewer reading the conversation saw nothing. This renders a
//! short digest of `ripr review-comments` output and decides whether to
//! create the comment or update the one an earlier run left. Nothing here
//! reads a token or the network.

use serde_json::{Value, json};

/// First line of the comment. Only a comment posted by the workflow's own
/// bot counts as ours, because anyone can type the marker.
pub(crate) const SUMMARY_MARKER: &str = "<!-- ripr:summary -->";
const WORKFLOW_BOT_LOGIN: &str = "github-actions[bot]";
/// Rows shown before the digest points at the full report.
const MAX_ROWS: usize = 5;
const MAX_CELL_CHARS: usize = 160;

/// What the workflow step should do with the comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SummaryRequest {
    pub(crate) method: &'static str,
    /// Endpoint after `repos/OWNER/REPO/`.
    pub(crate) endpoint: String,
    pub(crate) payload: Value,
    pub(crate) message: String,
}

/// The request for this run, or `None` when there is nothing to say and no
/// earlier comment to correct: a clean pull request gets no comment.
pub(crate) fn summary_request(
    guidance: &Value,
    pages: &Value,
    pull_request: &str,
    head_sha: &str,
    run_url: &str,
) -> Option<SummaryRequest> {
    let rows = guidance
        .get("comments")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    // The producer states summary-only items as a count and as a top-level
    // array; trust the larger so a missing count is never a false clean.
    let counted = guidance["summary"]["summary_only"].as_u64().unwrap_or(0);
    let listed = guidance
        .get("summary_only")
        .and_then(Value::as_array)
        .map_or(0, |items| items.len() as u64);
    let summary_only = counted.max(listed);
    let existing = existing_summary_id(pages);
    let incomplete = !analysis_complete(guidance);
    if rows.is_empty() && summary_only == 0 && !incomplete && existing.is_none() {
        return None;
    }
    let body = render_body(rows, summary_only, incomplete, head_sha, run_url);
    Some(match existing {
        Some(id) => SummaryRequest {
            method: "PATCH",
            endpoint: format!("issues/comments/{id}"),
            payload: json!({ "body": body }),
            message: format!("Updated RIPR summary comment {id}."),
        },
        None => SummaryRequest {
            method: "POST",
            endpoint: format!("issues/{pull_request}/comments"),
            payload: json!({ "body": body }),
            message: "Posted the RIPR summary comment.".to_string(),
        },
    })
}

/// Missing or false `analysis_complete` is not a clean result.
fn analysis_complete(guidance: &Value) -> bool {
    guidance["analysis_outcome"]["analysis_complete"].as_bool() == Some(true)
}

/// Id of the first marked comment the workflow bot posted.
fn existing_summary_id(pages: &Value) -> Option<u64> {
    let pages = pages.as_array()?;
    pages
        .iter()
        .flat_map(|page| page.as_array().map_or(&[][..], Vec::as_slice))
        .find_map(|comment| {
            let user = comment.get("user")?;
            let bot = user.get("login") == Some(&json!(WORKFLOW_BOT_LOGIN))
                && user.get("type") == Some(&json!("Bot"));
            let marked = comment["body"]
                .as_str()
                .is_some_and(|body| body.starts_with(SUMMARY_MARKER));
            if bot && marked {
                comment["id"].as_u64()
            } else {
                None
            }
        })
}

fn render_body(
    rows: &[Value],
    summary_only: u64,
    incomplete: bool,
    head_sha: &str,
    run_url: &str,
) -> String {
    let mut body = format!("{SUMMARY_MARKER}\n### RIPR test-evidence review\n\n");
    if rows.is_empty() && summary_only == 0 {
        if incomplete {
            body.push_str(
                "The analysis did not complete for this revision, so this is not a clean result. See the full report.\n",
            );
        } else {
            body.push_str(
                "RIPR found no weak-evidence spots in this revision (static, advisory; not a statement that the tests are sufficient).\n",
            );
        }
    } else {
        let total = rows.len() as u64 + summary_only;
        body.push_str(&format!(
            "{total} changed-behavior spot(s) where the tests may not notice a wrong result. Findings on changed lines also appear as check annotations in the Files tab.\n\n"
        ));
        if rows.is_empty() {
            body.push_str("The items are in the full report.\n");
        } else {
            body.push_str("| Where | Behavior | Missing check |\n| --- | --- | --- |\n");
        }
        for row in rows.iter().take(MAX_ROWS) {
            let place = format!(
                "{}:{}",
                text(&row["seam"]["file"]),
                row["seam"]["line"].as_u64().unwrap_or(0)
            );
            let owner = text(&row["owner"]);
            let discriminator = row["missing_discriminator"]
                .as_str()
                .unwrap_or("see the full report");
            body.push_str(&format!(
                "| {} | {} | {} |\n",
                code(&place),
                code(&format!("{} in {}", text(&row["kind"]), owner)),
                code(discriminator)
            ));
        }
        let hidden = total.saturating_sub(MAX_ROWS.min(rows.len()) as u64);
        if hidden > 0 {
            body.push_str(&format!("\n…and {hidden} more in the full report.\n"));
        }
        // Shown whole or not at all: a cleaned or cut command would run
        // something else when copied.
        if let Some(command) = rows
            .first()
            .and_then(|row| row["llm_guidance"]["repair_command"].as_str())
            .filter(|command| is_plain_command(command))
        {
            body.push_str(&format!("\nStart the top repair: `{command}`\n"));
        }
    }
    if let Some(url) = safe_url(run_url) {
        body.push_str(&format!("\n[Full report and artifacts]({url})\n"));
    }
    let sha: String = head_sha.chars().take(7).collect();
    body.push_str(&format!(
        "\n<sub>Advisory static evidence for {}. Not a mutation run, not coverage, and it does not block merging.</sub>\n",
        code(&sha)
    ));
    body
}

fn is_plain_command(command: &str) -> bool {
    command.len() <= 400
        && command.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '_' | '/' | ':' | '=' | '-')
        })
}

fn text(value: &Value) -> String {
    clean(value.as_str().unwrap_or("unknown"))
}

/// One line, no markup or mention can survive: the text comes from the
/// analyzed pull request.
fn clean(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars().take(MAX_CELL_CHARS) {
        match ch {
            '`' => out.push('\''),
            '|' => out.push('/'),
            '<' => out.push('('),
            '>' => out.push(')'),
            '@' => out.push_str("@\u{200b}"),
            '\r' | '\n' | '\t' => out.push(' '),
            // Controls and bidi/format marks that reorder or hide text.
            c if c.is_control()
                || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
                {}
            c => out.push(c),
        }
    }
    out
}

fn code(raw: &str) -> String {
    format!("`{}`", clean(raw))
}

/// Only the run URL GitHub gave the workflow: https, no whitespace or
/// markdown that could close the link early.
fn safe_url(url: &str) -> Option<&str> {
    let ok = url.starts_with("https://")
        && !url.contains(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '<' | '>' | '`'));
    ok.then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(file: &str, line: u64) -> Value {
        json!({
            "seam": { "file": file, "line": line },
            "owner": "src/lib.rs::discount",
            "kind": "predicate_boundary",
            "missing_discriminator": "5 (equality boundary)",
            "llm_guidance": { "repair_command": "ripr agent repair --root . --seam-id abc --phase before" },
        })
    }

    fn guidance(rows: Vec<Value>) -> Value {
        json!({ "comments": rows, "summary": { "summary_only": 0 }, "analysis_outcome": { "analysis_complete": true } })
    }

    const RUN: &str = "https://github.com/o/r/actions/runs/1";

    #[test]
    fn a_clean_pull_request_gets_no_comment() {
        let request = summary_request(&guidance(vec![]), &json!([]), "7", "abcdef0123", RUN);
        assert_eq!(request, None);
    }

    #[test]
    fn an_incomplete_analysis_is_never_reported_as_clean() -> Result<(), String> {
        let incomplete = json!({ "comments": [], "summary": { "summary_only": 0 }, "analysis_outcome": { "analysis_complete": false } });
        let request =
            summary_request(&incomplete, &json!([]), "7", "ab", RUN).ok_or("expected a comment")?;
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert!(body.contains("did not complete"));
        assert!(!body.contains("found no weak-evidence"));
        // An outcome the producer did not state is not a clean result either.
        let missing = json!({ "comments": [], "summary": { "summary_only": 0 } });
        assert!(summary_request(&missing, &json!([]), "7", "ab", RUN).is_some());
        Ok(())
    }

    #[test]
    fn a_repair_command_is_shown_whole_or_not_at_all() -> Result<(), String> {
        let mut long = row("a.rs", 1);
        long["llm_guidance"]["repair_command"] = json!(format!("ripr {}", "x".repeat(500)));
        let request = summary_request(&guidance(vec![long]), &json!([]), "7", "ab", RUN)
            .ok_or("expected a request")?;
        assert!(
            !request.payload["body"]
                .as_str()
                .ok_or("body")?
                .contains("Start the top repair")
        );
        let mut spoofed = row("a.rs", 1);
        spoofed["llm_guidance"]["repair_command"] = json!("ripr x; rm -rf ~");
        let request = summary_request(&guidance(vec![spoofed]), &json!([]), "7", "ab", RUN)
            .ok_or("expected a request")?;
        assert!(
            !request.payload["body"]
                .as_str()
                .ok_or("body")?
                .contains("rm -rf")
        );
        Ok(())
    }

    #[test]
    fn findings_create_one_comment_on_the_pull_request() -> Result<(), String> {
        let request = summary_request(
            &guidance(vec![row("src/lib.rs", 2)]),
            &json!([[]]),
            "7",
            "abcdef0123",
            RUN,
        )
        .ok_or("expected a request")?;
        assert_eq!(request.method, "POST");
        assert_eq!(request.endpoint, "issues/7/comments");
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert!(body.starts_with(SUMMARY_MARKER));
        assert!(body.contains("| `src/lib.rs:2` |"));
        assert!(body.contains("`5 (equality boundary)`"));
        assert!(body.contains("ripr agent repair"));
        assert!(body.contains(&format!("]({RUN})")));
        assert!(body.contains("`abcdef0`"));
        Ok(())
    }

    #[test]
    fn a_rerun_updates_the_bot_comment_even_when_the_findings_are_gone() -> Result<(), String> {
        let pages = json!([[{
            "id": 41, "body": format!("{SUMMARY_MARKER}\nold"),
            "user": { "login": "github-actions[bot]", "type": "Bot" }
        }]]);
        let request = summary_request(&guidance(vec![]), &pages, "7", "abcdef0", RUN)
            .ok_or("expected an update")?;
        assert_eq!(request.method, "PATCH");
        assert_eq!(request.endpoint, "issues/comments/41");
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert!(body.contains("found no weak-evidence spots"));
        assert!(body.contains("not a statement that the tests are sufficient"));
        Ok(())
    }

    #[test]
    fn a_forged_marker_from_another_author_is_never_updated() -> Result<(), String> {
        let forged = |login: &str, kind: &str| {
            json!([[{
                "id": 9, "body": format!("{SUMMARY_MARKER}\nhi"),
                "user": { "login": login, "type": kind }
            }]])
        };
        for pages in [
            forged("mallory", "User"),
            forged("github-actions[bot]", "User"),
            forged("other-app[bot]", "Bot"),
        ] {
            let request = summary_request(&guidance(vec![row("a.rs", 1)]), &pages, "7", "ab", RUN)
                .ok_or("expected a request")?;
            assert_eq!(request.method, "POST");
        }
        Ok(())
    }

    #[test]
    fn rows_are_capped_and_the_rest_is_counted() -> Result<(), String> {
        let rows = (1..=8).map(|n| row("src/lib.rs", n)).collect();
        let request = summary_request(&guidance(rows), &json!([]), "7", "ab", RUN)
            .ok_or("expected a request")?;
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert_eq!(body.matches("| `src/lib.rs:").count(), MAX_ROWS);
        assert!(body.contains("…and 3 more"));
        Ok(())
    }

    #[test]
    fn text_from_the_analyzed_change_cannot_inject_markup_or_mentions() -> Result<(), String> {
        let mut hostile = row("a|b`.rs", 1);
        hostile["missing_discriminator"] = json!("x`\n@admin <img src=x> | [l](u)");
        let request = summary_request(
            &guidance(vec![hostile]),
            &json!([]),
            "7",
            "ab",
            "https://x.test/a b)",
        )
        .ok_or("expected a request")?;
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert!(!body.contains("<img"));
        assert!(!body.contains("@admin"));
        assert!(!body.contains("x.test"));
        assert!(body.contains("| `a/b'.rs:1` | `predicate_boundary in src/lib.rs::discount` | `x' @\u{200b}admin (img src=x) / [l](u)` |"));
        Ok(())
    }

    #[test]
    fn an_array_only_summary_is_not_reported_as_clean() -> Result<(), String> {
        let g = json!({ "comments": [], "summary": {}, "summary_only": [{"a": 1}, {"a": 2}], "analysis_outcome": { "analysis_complete": true } });
        let request =
            summary_request(&g, &json!([]), "7", "ab", RUN).ok_or("expected a comment")?;
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert!(body.contains("2 changed-behavior spot(s)"));
        assert!(body.contains("The items are in the full report."));
        assert!(!body.contains("| --- |"));
        assert!(!body.contains("found no weak-evidence"));
        Ok(())
    }

    #[test]
    fn summary_only_items_are_counted_beyond_the_shown_rows() -> Result<(), String> {
        let g = json!({ "comments": [row("a.rs", 1)], "summary": { "summary_only": 3 }, "analysis_outcome": { "analysis_complete": true } });
        let request =
            summary_request(&g, &json!([]), "7", "ab", RUN).ok_or("expected a comment")?;
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert!(body.contains("4 changed-behavior spot(s)"));
        assert!(body.contains("…and 3 more"));
        Ok(())
    }
}
