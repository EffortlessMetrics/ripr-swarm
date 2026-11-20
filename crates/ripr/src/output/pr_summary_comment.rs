//! The one pull-request conversation comment of the generated workflow
//! (`ripr pr-comments summary`).
//!
//! The job summary lives on the Actions run page and the annotations on the
//! Files tab; a reviewer reading the conversation saw nothing. This renders a
//! short digest of `ripr review-comments` output and decides whether to
//! create the comment or update the one an earlier run left. Nothing here
//! reads a token or the network.
//!
//! #15718: with a blocking `RIPR_GATE_MODE` the job is not advisory, so the
//! comment states a failing gate verdict first and never says RIPR does not
//! block merging.

use crate::output::ci_summary::gate_verdict::GateVerdict;
use serde_json::{Value, json};

/// First line of the comment. Only a comment posted by the workflow's own
/// bot counts as ours, because anyone can type the marker.
pub(crate) const SUMMARY_MARKER: &str = "<!-- ripr:summary -->";
const WORKFLOW_BOT_LOGIN: &str = "github-actions[bot]";
/// Rows shown before the digest points at the full report.
const MAX_ROWS: usize = 5;
const MAX_CELL_CHARS: usize = 160;

/// The gate facts of this run the comment depends on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GateState {
    /// `RIPR_GATE_MODE` as the job read it; empty is the advisory default.
    pub(crate) mode: String,
    /// The gate's failing verdict for this run, if any.
    pub(crate) verdict: Option<GateVerdict>,
}

impl GateState {
    /// Whether the job can fail on the gate: the generated workflow sets
    /// `continue-on-error` only for an empty mode and `visible-only`, so any
    /// other value, including one the gate refuses, can block merging.
    fn can_block(&self) -> bool {
        !self.mode.is_empty() && self.mode != "visible-only"
    }
}

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
    gate: &GateState,
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
    // A failing gate is news on its own, even with no finding rows.
    if rows.is_empty()
        && summary_only == 0
        && !incomplete
        && existing.is_none()
        && gate.verdict.is_none()
    {
        return None;
    }
    let body = render_body(rows, summary_only, incomplete, head_sha, run_url, gate);
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
    gate: &GateState,
) -> String {
    let mut body = format!("{SUMMARY_MARKER}\n### RIPR test-evidence review\n\n");
    if let Some(verdict) = &gate.verdict {
        body.push_str(&verdict.comment_line(code));
        body.push_str("\n\n");
    }
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
    if gate.can_block() {
        body.push_str(&format!(
            "\n<sub>Static evidence for {}. Not a mutation run and not coverage. This check runs the RIPR gate with `RIPR_GATE_MODE` set to {}, so it can block merging.</sub>\n",
            code(&sha),
            code(&gate.mode)
        ));
    } else {
        body.push_str(&format!(
            "\n<sub>Advisory static evidence for {}. Not a mutation run, not coverage, and it does not block merging.</sub>\n",
            code(&sha)
        ));
    }
    body
}

pub(crate) fn is_plain_command(command: &str) -> bool {
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
        let request = summary_request(
            &guidance(vec![]),
            &json!([]),
            "7",
            "abcdef0123",
            RUN,
            &GateState::default(),
        );
        assert_eq!(request, None);
    }

    #[test]
    fn an_incomplete_analysis_is_never_reported_as_clean() -> Result<(), String> {
        let incomplete = json!({ "comments": [], "summary": { "summary_only": 0 }, "analysis_outcome": { "analysis_complete": false } });
        let request = summary_request(
            &incomplete,
            &json!([]),
            "7",
            "ab",
            RUN,
            &GateState::default(),
        )
        .ok_or("expected a comment")?;
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert!(body.contains("did not complete"));
        assert!(!body.contains("found no weak-evidence"));
        // An outcome the producer did not state is not a clean result either.
        let missing = json!({ "comments": [], "summary": { "summary_only": 0 } });
        assert!(
            summary_request(&missing, &json!([]), "7", "ab", RUN, &GateState::default()).is_some()
        );
        Ok(())
    }

    #[test]
    fn a_repair_command_is_shown_whole_or_not_at_all() -> Result<(), String> {
        let mut long = row("a.rs", 1);
        long["llm_guidance"]["repair_command"] = json!(format!("ripr {}", "x".repeat(500)));
        let request = summary_request(
            &guidance(vec![long]),
            &json!([]),
            "7",
            "ab",
            RUN,
            &GateState::default(),
        )
        .ok_or("expected a request")?;
        assert!(
            !request.payload["body"]
                .as_str()
                .ok_or("body")?
                .contains("Start the top repair")
        );
        let mut spoofed = row("a.rs", 1);
        spoofed["llm_guidance"]["repair_command"] = json!("ripr x; rm -rf ~");
        let request = summary_request(
            &guidance(vec![spoofed]),
            &json!([]),
            "7",
            "ab",
            RUN,
            &GateState::default(),
        )
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
            &GateState::default(),
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
        let request = summary_request(
            &guidance(vec![]),
            &pages,
            "7",
            "abcdef0",
            RUN,
            &GateState::default(),
        )
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
            let request = summary_request(
                &guidance(vec![row("a.rs", 1)]),
                &pages,
                "7",
                "ab",
                RUN,
                &GateState::default(),
            )
            .ok_or("expected a request")?;
            assert_eq!(request.method, "POST");
        }
        Ok(())
    }

    #[test]
    fn rows_are_capped_and_the_rest_is_counted() -> Result<(), String> {
        let rows = (1..=8).map(|n| row("src/lib.rs", n)).collect();
        let request = summary_request(
            &guidance(rows),
            &json!([]),
            "7",
            "ab",
            RUN,
            &GateState::default(),
        )
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
            &GateState::default(),
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
        let request = summary_request(&g, &json!([]), "7", "ab", RUN, &GateState::default())
            .ok_or("expected a comment")?;
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
        let request = summary_request(&g, &json!([]), "7", "ab", RUN, &GateState::default())
            .ok_or("expected a comment")?;
        let body = request.payload["body"].as_str().ok_or("body")?;
        assert!(body.contains("4 changed-behavior spot(s)"));
        assert!(body.contains("…and 3 more"));
        Ok(())
    }

    fn gate(mode: &str, verdict: Option<GateVerdict>) -> GateState {
        GateState {
            mode: mode.to_string(),
            verdict,
        }
    }

    fn blocked(place: &str) -> GateVerdict {
        GateVerdict::Blocked {
            mode: "acknowledgeable".to_string(),
            blocking: 2,
            first: Some((
                "weak oracle on changed behavior".to_string(),
                Some(place.to_string()),
            )),
            exception: None,
        }
    }

    fn body_for(g: &Value, state: &GateState) -> Result<String, String> {
        let request = summary_request(g, &json!([]), "7", "abcdef0123", RUN, state)
            .ok_or("expected a comment")?;
        Ok(request.payload["body"].as_str().ok_or("body")?.to_string())
    }

    /// #15718: only an advisory job says it does not block merging; every
    /// mode the workflow does not mark `continue-on-error` says it can.
    #[test]
    fn the_footer_says_it_can_block_merging_exactly_when_the_job_can_fail() -> Result<(), String> {
        let findings = guidance(vec![row("src/lib.rs", 2)]);
        for advisory in ["", "visible-only"] {
            let body = body_for(&findings, &gate(advisory, None))?;
            assert!(
                body.contains("and it does not block merging."),
                "{advisory}: {body}"
            );
            assert!(!body.contains("can block merging"), "{advisory}: {body}");
        }
        for blocking in [
            "acknowledgeable",
            "baseline-check",
            "calibrated-gate",
            "visible_only",
        ] {
            let body = body_for(&findings, &gate(blocking, None))?;
            assert!(
                !body.contains("does not block merging"),
                "{blocking}: {body}"
            );
            assert!(
                body.contains(&format!(
                    "with `RIPR_GATE_MODE` set to `{blocking}`, so it can block merging."
                )),
                "{blocking}: {body}"
            );
        }
        Ok(())
    }

    /// #15718: a blocked gate leads the comment, before the finding table,
    /// and is posted even when no finding row would be.
    #[test]
    fn a_blocked_gate_is_stated_first_and_always_posted() -> Result<(), String> {
        let state = gate("acknowledgeable", Some(blocked("src/lib.rs:2")));
        let body = body_for(&guidance(vec![row("src/lib.rs", 2)]), &state)?;
        let verdict = "**The RIPR gate blocked this pull request** under `acknowledgeable` mode: 2 blocking gaps. First: `weak oracle on changed behavior` at `src/lib.rs:2`.";
        let at = body.find(verdict).ok_or(body.clone())?;
        let table = body.find("| Where |").ok_or(body.clone())?;
        assert!(at < table, "{body}");
        assert!(body.starts_with(&format!(
            "{SUMMARY_MARKER}\n### RIPR test-evidence review\n\n{verdict}"
        )));

        // No rows and a complete analysis: still a comment, because of the gate.
        assert_eq!(
            summary_request(
                &guidance(vec![]),
                &json!([]),
                "7",
                "ab",
                RUN,
                &gate("acknowledgeable", None)
            ),
            None
        );
        let body = body_for(&guidance(vec![]), &state)?;
        assert!(body.contains(verdict), "{body}");
        Ok(())
    }

    #[test]
    fn a_config_error_and_an_unknown_mode_are_named_as_setup_problems() -> Result<(), String> {
        let config = GateVerdict::ConfigError {
            mode: "baseline-check".to_string(),
            errors: vec!["RIPR_GATE_BASELINE is not set".to_string()],
        };
        let body = body_for(&guidance(vec![]), &gate("baseline-check", Some(config)))?;
        assert!(body.contains("**The RIPR gate could not evaluate this pull request** under `baseline-check` mode. This is a setup problem, not a finding about the code. First error: `RIPR_GATE_BASELINE is not set`."), "{body}");

        let unknown = GateVerdict::from_mode("visible_only").ok_or("refused mode")?;
        let body = body_for(&guidance(vec![]), &gate("visible_only", Some(unknown)))?;
        assert!(
            body.contains(
                "**The RIPR gate did not run**, because `RIPR_GATE_MODE` is not a gate mode: `"
            ),
            "{body}"
        );
        assert!(body.contains("unset it to stay advisory"), "{body}");
        assert!(
            body.contains("set to `visible_only`, so it can block merging."),
            "{body}"
        );
        Ok(())
    }

    #[test]
    fn gate_text_from_the_analyzed_change_cannot_inject_markup_or_mentions() -> Result<(), String> {
        let state = gate("acknowledgeable", Some(blocked("a`b|@admin <img>.rs:1")));
        let body = body_for(&guidance(vec![]), &state)?;
        assert!(!body.contains("<img"), "{body}");
        assert!(!body.contains("@admin"), "{body}");
        assert!(
            body.contains("at `a'b/@\u{200b}admin (img).rs:1`."),
            "{body}"
        );
        let hostile_mode = gate("x`\n@team", None);
        let body = body_for(&guidance(vec![row("a.rs", 1)]), &hostile_mode)?;
        assert!(body.contains("set to `x' @\u{200b}team`"), "{body}");
        Ok(())
    }
}
