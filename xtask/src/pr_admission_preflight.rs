//! Local mirror of the `pr-candidate-admission` gates for `cargo xtask pr-ready --pr <N>` (#14639).
//!
//! Advisory preflight only: the hosted action in `.github/actions/pr-candidate-admission/`
//! remains the authority. This refuses early, with remediation, on the failures that
//! otherwise burn a `Routed Rust Small` / `merge_group` run.
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::Command;

pub(crate) const MAX_CHANGED_FILES: u64 = 160;
pub(crate) const MAX_CHANGED_LINES: u64 = 3000;

const CLAIM_LABELS: [&str; 2] = [
    "governing issue or accepted claim",
    "owner / governing claim",
];
const DELTA_LABELS: [&str; 2] = ["acceptance delta (one sentence)", "acceptance delta"];
const PLACEHOLDERS: [&str; 10] = [
    "",
    "n/a",
    "na",
    "none",
    "not applicable",
    "tbd",
    "todo",
    "unknown",
    "pending",
    "-",
];

/// Parse `--pr <N>` / `--pr=<N>`. `Ok(None)` when absent; unknown arguments are refused.
pub(crate) fn parse_pr_arg(args: &[String]) -> Result<Option<u64>, String> {
    let mut pr = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let value = if arg == "--pr" {
            iter.next()
                .ok_or("--pr requires a pull request number")?
                .as_str()
        } else if let Some(value) = arg.strip_prefix("--pr=") {
            value
        } else {
            return Err(format!(
                "unknown pr-ready argument `{arg}`; usage: cargo xtask pr-ready [--pr <number>]"
            ));
        };
        let number = value
            .parse::<u64>()
            .ok()
            .filter(|number| *number > 0)
            .ok_or_else(|| format!("invalid pull request number `{value}`"))?;
        pr = Some(number);
    }
    Ok(pr)
}

fn is_placeholder(value: &str) -> bool {
    let stripped = value.trim().trim_matches('`').trim();
    let normalized = stripped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    normalized.starts_with('<') || PLACEHOLDERS.contains(&normalized.as_str()) || normalized == "—"
}

/// Lines outside fenced code blocks and HTML comments.
fn visible_lines(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fenced = false;
    let mut comment = false;
    for raw in body.lines() {
        let mut line = raw.to_string();
        if comment {
            match line.find("-->") {
                Some(end) => {
                    line = line[end + 3..].to_string();
                    comment = false;
                }
                None => continue,
            }
        }
        while let Some(start) = line.find("<!--") {
            match line[start..].find("-->") {
                Some(end) => line.replace_range(start..start + end + 3, ""),
                None => {
                    line.truncate(start);
                    comment = true;
                }
            }
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            out.push(line);
        }
    }
    out
}

fn field_value(lines: &[String], labels: &[&str]) -> Option<String> {
    lines.iter().find_map(|line| {
        let trimmed = line.trim();
        labels.iter().find_map(|label| {
            let prefix = format!("{label}:");
            trimmed
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(&prefix))
                .then(|| trimmed.get(prefix.len()..).unwrap_or("").trim().to_string())
        })
    })
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Exactly one visible `Source review: [SOURCE_]REVIEW_READY <head>` line.
fn body_attests_head(lines: &[String], head: &str) -> bool {
    if !is_full_sha(head) {
        return false;
    }
    let values: Vec<String> = lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim();
            let prefix = "source review:";
            trimmed
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
                .then(|| trimmed.get(prefix.len()..).unwrap_or("").trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .collect();
    let [value] = values.as_slice() else {
        return false;
    };
    ["SOURCE_REVIEW_READY ", "REVIEW_READY "]
        .iter()
        .any(|marker| value.strip_prefix(marker) == Some(head))
}

fn is_trusted_association(value: Option<&str>) -> bool {
    matches!(
        value.unwrap_or("").to_ascii_uppercase().as_str(),
        "OWNER" | "MEMBER" | "COLLABORATOR"
    )
}

fn comment_review_ready(body: &str) -> bool {
    let visible = visible_lines(body);
    let Some(first) = visible
        .iter()
        .map(|line| line.trim())
        .find(|l| !l.is_empty())
    else {
        return false;
    };
    let normalized = first
        .trim_matches(|c: char| matches!(c, '*' | '_' | '`' | '-' | ' '))
        .to_ascii_uppercase();
    let has_ready_marker = [
        "SOURCE_REVIEW_READY",
        "REVIEW_READY",
        "DISPOSITION: SOURCE_REVIEW_READY",
        "DISPOSITION: REVIEW_READY",
    ]
    .iter()
    .any(|marker| {
        normalized == *marker
            || normalized
                .strip_prefix(marker)
                .is_some_and(|rest| rest.starts_with([' ', ':']))
    });
    if !has_ready_marker {
        return false;
    }
    !visible.iter().any(|line| {
        let upper = line.to_ascii_uppercase();
        upper.contains("REVIEW_INCOMPLETE") || upper.contains("CHANGES_REQUESTED")
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReviewStatus {
    Blocked,
    Attested,
    None,
}

/// Mirror `reviews_allow_source_attestation` for `gh pr view --json reviews`:
/// latest decisive review per login cannot be `CHANGES_REQUESTED`, and either a trusted
/// `APPROVED` review on `head` or a trusted latest `COMMENTED` review on `head` starting
/// with `SOURCE_REVIEW_READY` / `REVIEW_READY` attests `head`.
fn review_status(reviews: &Value, head: &str) -> ReviewStatus {
    let Some(items) = reviews.as_array() else {
        return ReviewStatus::None;
    };
    let mut latest_decisive: BTreeMap<String, &Value> = BTreeMap::new();
    let mut latest_all: BTreeMap<String, &Value> = BTreeMap::new();
    for (idx, review) in items.iter().enumerate() {
        let login = review["author"]["login"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("<anonymous:{idx}>"));
        let state = review["state"].as_str().unwrap_or("");
        if matches!(state, "APPROVED" | "CHANGES_REQUESTED") {
            latest_decisive.insert(login.clone(), review);
        }
        latest_all.insert(login, review);
    }
    if latest_decisive
        .values()
        .any(|r| r["state"].as_str() == Some("CHANGES_REQUESTED"))
    {
        return ReviewStatus::Blocked;
    }
    if !is_full_sha(head) {
        return ReviewStatus::None;
    }
    if latest_decisive.values().any(|r| {
        r["state"].as_str() == Some("APPROVED")
            && r["commit"]["oid"].as_str() == Some(head)
            && is_trusted_association(r["authorAssociation"].as_str())
    }) {
        return ReviewStatus::Attested;
    }
    if latest_all.values().any(|r| {
        r["state"].as_str() == Some("COMMENTED")
            && r["commit"]["oid"].as_str() == Some(head)
            && is_trusted_association(r["authorAssociation"].as_str())
            && comment_review_ready(r["body"].as_str().unwrap_or(""))
    }) {
        return ReviewStatus::Attested;
    }
    ReviewStatus::None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) gate: &'static str,
    pub(crate) problem: String,
    pub(crate) remediation: String,
}

/// Evaluate the admission contract over a `gh pr view` snapshot and the unresolved
/// review-thread count (`None` = could not be established, which is refused).
pub(crate) fn evaluate(pr: &Value, unresolved_threads: Option<u64>) -> Vec<Finding> {
    let mut findings = Vec::new();
    let body = pr["body"].as_str().unwrap_or("");
    let lines = visible_lines(body);
    let number = pr["number"].as_u64().unwrap_or(0);

    if field_value(&lines, &CLAIM_LABELS).is_none_or(|value| is_placeholder(&value)) {
        findings.push(Finding {
            gate: "claim",
            problem: "PR body lacks a filled `Governing issue or accepted claim:` line".into(),
            remediation: format!(
                "gh pr edit {number} --body-file <file> with `Governing issue or accepted claim: #<issue>`"
            ),
        });
    }
    if field_value(&lines, &DELTA_LABELS).is_none_or(|value| is_placeholder(&value)) {
        findings.push(Finding {
            gate: "acceptance_delta",
            problem: "PR body lacks a filled `Acceptance delta (one sentence):` line".into(),
            remediation: format!(
                "gh pr edit {number} --body-file <file> with `Acceptance delta (one sentence): <what is now accepted>`"
            ),
        });
    }

    let head = pr["headRefOid"].as_str().unwrap_or("");
    let attested = match review_status(&pr["reviews"], head) {
        ReviewStatus::Blocked => false,
        ReviewStatus::Attested => true,
        ReviewStatus::None => body_attests_head(&lines, head),
    };
    if !attested {
        findings.push(Finding {
            gate: "review_disposition",
            problem: format!(
                "no exact-head attestation for {head}; the head needs `Source review: REVIEW_READY <head>` in the body (exactly one line) or a trusted approving/SOURCE_REVIEW_READY review"
            ),
            remediation: format!(
                "after review-pr on this exact head, post a native COMMENT review starting with `SOURCE_REVIEW_READY` (last, after any thread replies) or add `Source review: REVIEW_READY {head}` to the PR body"
            ),
        });
    }

    match unresolved_threads {
        Some(0) => {}
        Some(count) => findings.push(Finding {
            gate: "review_threads",
            problem: format!("{count} unresolved review thread(s)"),
            remediation: "reply to and resolve each thread in the PR Conversation after verifying or repairing the finding".into(),
        }),
        None => findings.push(Finding {
            gate: "review_threads",
            problem: "unresolved review threads could not be established".into(),
            remediation: "re-run with working `gh` auth, or inspect the PR Conversation manually".into(),
        }),
    }

    let files = pr["changedFiles"].as_u64();
    let lines_changed = pr["additions"]
        .as_u64()
        .zip(pr["deletions"].as_u64())
        .map(|(a, d)| a + d);
    match (files, lines_changed) {
        (Some(files), Some(changed)) => {
            if files > MAX_CHANGED_FILES || changed > MAX_CHANGED_LINES {
                findings.push(Finding {
                    gate: "diff_cap",
                    problem: format!(
                        "diff is {files} files / {changed} lines; cap is {MAX_CHANGED_FILES} files / {MAX_CHANGED_LINES} lines"
                    ),
                    remediation: "split into narrower PR-sized claims before requesting merge-ready".into(),
                });
            }
        }
        _ => findings.push(Finding {
            gate: "diff_cap",
            problem: "changed file/line counts missing from PR snapshot".into(),
            remediation: "re-run with working `gh` auth".into(),
        }),
    }
    findings
}

pub(crate) fn render(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("[{}] {} -> {}", f.gate, f.problem, f.remediation))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Single-line summary so `repo_readiness::first_error_line` preserves all failing gates
/// and remediations in `target/ripr/reports/pr-ready.{md,json}`.
pub(crate) fn render_one_line(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("[{}] {} -> {}", f.gate, f.problem, f.remediation))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn gh(args: &[&str]) -> Result<String, String> {
    let output = Command::new("gh")
        .args(args)
        .output()
        .map_err(|err| format!("failed to run gh: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "gh {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

const THREADS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){repository(owner:$owner,name:$repo){pullRequest(number:$number){reviewThreads(first:100){pageInfo{hasNextPage} nodes{isResolved}}}}}";

/// `Some(n)` unresolved threads, or `None` when the answer is incomplete (pagination/shape).
pub(crate) fn unresolved_threads(response: &Value) -> Option<u64> {
    let threads = &response["data"]["repository"]["pullRequest"]["reviewThreads"];
    if response.get("errors").is_some()
        || threads["pageInfo"]["hasNextPage"].as_bool() != Some(false)
    {
        return None;
    }
    let nodes = threads["nodes"].as_array()?;
    Some(
        nodes
            .iter()
            .filter(|node| node["isResolved"].as_bool() != Some(true))
            .count() as u64,
    )
}

/// Run the preflight for a PR. `Err` carries the remediation list.
pub(crate) fn run(number: u64) -> Result<(), String> {
    let number_text = number.to_string();
    let view = gh(&[
        "pr",
        "view",
        &number_text,
        "--json",
        "number,body,headRefOid,changedFiles,additions,deletions,reviews",
    ])?;
    let pr: Value =
        serde_json::from_str(&view).map_err(|err| format!("invalid gh pr view JSON: {err}"))?;
    if pr["number"].as_u64() != Some(number) {
        return Err(format!("PR identity mismatch: expected #{number}"));
    }
    let threads = gh(&[
        "api",
        "graphql",
        "-F",
        "owner={owner}",
        "-F",
        "repo={repo}",
        "-F",
        &format!("number={number}"),
        "-f",
        &format!("query={THREADS_QUERY}"),
    ])
    .ok()
    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    .and_then(|value| unresolved_threads(&value));
    let findings = evaluate(&pr, threads);
    if findings.is_empty() {
        Ok(())
    } else {
        eprintln!(
            "pr-candidate-admission preflight refused PR #{number}:\n{}",
            render(&findings)
        );
        Err(format!(
            "PR #{number} refused: {}",
            render_one_line(&findings)
        ))
    }
}

#[cfg(test)]
mod pr_ready_tests {
    use super::*;
    use serde_json::json;

    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

    fn pr(body: &str) -> Value {
        json!({"number": 7, "body": body, "headRefOid": HEAD,
               "changedFiles": 3, "additions": 10, "deletions": 5})
    }

    fn good_body() -> String {
        format!(
            "Governing issue or accepted claim: #14639\nAcceptance delta (one sentence): preflight refuses early.\nSource review: REVIEW_READY {HEAD}\n"
        )
    }

    fn gates(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.gate).collect()
    }

    #[test]
    fn pr_ready_accepts_complete_candidate() {
        assert!(evaluate(&pr(&good_body()), Some(0)).is_empty());
    }

    #[test]
    fn pr_ready_refuses_missing_claim_and_delta() {
        let found = evaluate(&pr(&format!("Source review: REVIEW_READY {HEAD}")), Some(0));
        assert_eq!(gates(&found), ["claim", "acceptance_delta"]);
        assert!(found[0].remediation.contains("gh pr edit 7"));
    }

    #[test]
    fn pr_ready_refuses_placeholder_and_template_values() {
        let body = format!(
            "Governing issue or accepted claim: TBD\nAcceptance delta (one sentence): <describe>\nSource review: REVIEW_READY {HEAD}"
        );
        assert_eq!(
            gates(&evaluate(&pr(&body), Some(0))),
            ["claim", "acceptance_delta"]
        );
    }

    #[test]
    fn pr_ready_ignores_fenced_and_commented_fields() {
        let body = format!(
            "```\nGoverning issue or accepted claim: #1\nAcceptance delta: x\nSource review: REVIEW_READY {HEAD}\n```\n<!-- Governing issue or accepted claim: #1 -->\n"
        );
        assert_eq!(
            gates(&evaluate(&pr(&body), Some(0))),
            ["claim", "acceptance_delta", "review_disposition"]
        );
    }

    #[test]
    fn pr_ready_requires_exact_head_attestation() {
        let stale = good_body().replace(HEAD, &"a".repeat(40));
        assert_eq!(
            gates(&evaluate(&pr(&stale), Some(0))),
            ["review_disposition"]
        );
        let doubled = format!("{}Source review: REVIEW_READY {HEAD}\n", good_body());
        assert_eq!(
            gates(&evaluate(&pr(&doubled), Some(0))),
            ["review_disposition"]
        );
        let alt = good_body().replace("REVIEW_READY", "SOURCE_REVIEW_READY");
        assert!(evaluate(&pr(&alt), Some(0)).is_empty());
    }

    #[test]
    fn pr_ready_refuses_unresolved_or_unknown_threads() {
        assert_eq!(
            gates(&evaluate(&pr(&good_body()), Some(2))),
            ["review_threads"]
        );
        assert_eq!(
            gates(&evaluate(&pr(&good_body()), None)),
            ["review_threads"]
        );
    }

    #[test]
    fn pr_ready_enforces_diff_cap_boundaries() {
        let mut at_cap = pr(&good_body());
        at_cap["changedFiles"] = json!(160);
        at_cap["additions"] = json!(2000);
        at_cap["deletions"] = json!(1000);
        assert!(evaluate(&at_cap, Some(0)).is_empty());
        at_cap["deletions"] = json!(1001);
        assert_eq!(gates(&evaluate(&at_cap, Some(0))), ["diff_cap"]);
        at_cap["deletions"] = json!(0);
        at_cap["changedFiles"] = json!(161);
        assert_eq!(gates(&evaluate(&at_cap, Some(0))), ["diff_cap"]);
        let mut missing = pr(&good_body());
        missing.as_object_mut().map(|o| o.remove("additions"));
        assert_eq!(gates(&evaluate(&missing, Some(0))), ["diff_cap"]);
    }

    #[test]
    fn pr_ready_thread_response_must_be_complete() {
        let page = |next: bool, resolved: &[bool]| {
            json!({"data":{"repository":{"pullRequest":{"reviewThreads":{
                "pageInfo":{"hasNextPage":next},
                "nodes": resolved.iter().map(|r| json!({"isResolved": r})).collect::<Vec<_>>()}}}}})
        };
        assert_eq!(
            unresolved_threads(&page(false, &[true, false, false])),
            Some(2)
        );
        assert_eq!(unresolved_threads(&page(false, &[])), Some(0));
        assert_eq!(unresolved_threads(&page(true, &[true])), None);
        assert_eq!(unresolved_threads(&json!({"errors":[{}]})), None);
    }

    #[test]
    fn pr_ready_parses_pr_argument() -> Result<(), String> {
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_pr_arg(&[]), Ok(None));
        assert_eq!(parse_pr_arg(&args(&["--pr", "42"])), Ok(Some(42)));
        assert_eq!(parse_pr_arg(&args(&["--pr=42"])), Ok(Some(42)));
        for invalid in [
            args(&["--pr"]),
            args(&["--pr", "0"]),
            args(&["--pr", "x"]),
            args(&["--bogus"]),
        ] {
            let Err(_) = parse_pr_arg(&invalid) else {
                return Err(format!("expected error for {invalid:?}"));
            };
        }
        Ok(())
    }

    #[test]
    fn pr_ready_handles_multibyte_utf8_and_backtick_placeholders() {
        let utf8_body = format!(
            "0123456789abc\u{2014}def\nGoverning issue or accepted claim: `TBD`\nAcceptance delta (one sentence): `<describe>`\nİ\nSource review: REVIEW_READY {HEAD}\n"
        );
        assert_eq!(
            gates(&evaluate(&pr(&utf8_body), Some(0))),
            ["claim", "acceptance_delta"]
        );
    }

    #[test]
    fn pr_ready_accepts_native_reviews_and_rejects_trailing_empty_comment() {
        let base_body =
            "Governing issue or accepted claim: #14639\nAcceptance delta (one sentence): ready.\n";
        let mut with_comment = pr(base_body);
        with_comment["reviews"] = json!([{
            "author": {"login": "verifier"},
            "authorAssociation": "COLLABORATOR",
            "state": "COMMENTED",
            "commit": {"oid": HEAD},
            "body": "SOURCE_REVIEW_READY\n"
        }]);
        assert!(evaluate(&with_comment, Some(0)).is_empty());

        let mut with_trailing_reply = pr(base_body);
        with_trailing_reply["reviews"] = json!([
            {
                "author": {"login": "verifier"},
                "authorAssociation": "COLLABORATOR",
                "state": "COMMENTED",
                "commit": {"oid": HEAD},
                "body": "SOURCE_REVIEW_READY\n"
            },
            {
                "author": {"login": "verifier"},
                "authorAssociation": "COLLABORATOR",
                "state": "COMMENTED",
                "commit": {"oid": HEAD},
                "body": ""
            }
        ]);
        assert_eq!(
            gates(&evaluate(&with_trailing_reply, Some(0))),
            ["review_disposition"]
        );

        let mut with_approved = pr(base_body);
        with_approved["reviews"] = json!([{
            "author": {"login": "maintainer"},
            "authorAssociation": "OWNER",
            "state": "APPROVED",
            "commit": {"oid": HEAD},
            "body": ""
        }]);
        assert!(evaluate(&with_approved, Some(0)).is_empty());

        let mut with_changes_requested = pr(&good_body());
        with_changes_requested["reviews"] = json!([{
            "author": {"login": "maintainer"},
            "authorAssociation": "OWNER",
            "state": "CHANGES_REQUESTED",
            "commit": {"oid": HEAD},
            "body": "fix"
        }]);
        assert_eq!(
            gates(&evaluate(&with_changes_requested, Some(0))),
            ["review_disposition"]
        );
    }

    #[test]
    fn pr_ready_render_one_line_preserves_all_findings() {
        let found = evaluate(&pr(""), Some(2));
        let line = render_one_line(&found);
        assert!(!line.contains('\n'));
        assert!(line.contains("[claim]"));
        assert!(line.contains("[acceptance_delta]"));
        assert!(line.contains("[review_disposition]"));
        assert!(line.contains("[review_threads]"));
    }
}
