//! Observed provider responses, not a review-completion or approval authority.
//!
//! A packet head identifies when these historical responses were observed. It
//! does not attribute issue comments or reviews to that commit. Review subject
//! currentness is a separate contract owned by gh-pr-status's review records.

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Disposition {
    QuotaExhausted,
    SubstanceUnknown,
}

impl Disposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::QuotaExhausted => "quota_exhausted",
            Self::SubstanceUnknown => "substance_unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Response {
    author: String,
    source_kind: &'static str,
    state: String,
    timestamp: String,
    url: String,
    body: String,
    disposition: Disposition,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProviderReviewObservations {
    snapshot_head: String,
    responses: Vec<Response>,
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn known_provider(author: &str) -> bool {
    matches!(
        author.strip_suffix("[bot]").unwrap_or(author),
        "sourcery-ai" | "gemini-code-assist" | "chatgpt-codex-connector" | "coderabbitai"
    )
}

fn classify(author: &str, body: &str) -> Disposition {
    let body = body.trim_start_matches(['\r', '\n']);
    // Match the provider's leading notice, never arbitrary quota prose inside
    // a substantive review, quoted example or code fence. Keep stored bytes raw.
    let quota = match author.strip_suffix("[bot]").unwrap_or(author) {
        "chatgpt-codex-connector" => {
            body.starts_with("You have reached your Codex usage limits for code reviews.")
        }
        "gemini-code-assist" => {
            let mut lines = body.lines();
            lines.next().map(str::trim_end) == Some("> [!WARNING]")
                && lines.next().is_some_and(|line| {
                    line.trim_end()
                        .starts_with("> You have reached your daily quota limit.")
                })
        }
        "sourcery-ai" => body.strip_prefix("Sorry @").is_some_and(|tail| {
            let Some((login, rest)) =
                tail.split_once(", you have reached your weekly rate limit of ")
            else {
                return false;
            };
            !login.is_empty()
                && login
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && rest
                    .split_once(" diff characters.")
                    .is_some_and(|(amount, _)| {
                        !amount.is_empty() && amount.bytes().all(|b| b.is_ascii_digit())
                    })
        }),
        "coderabbitai" => {
            let mut lines = body.lines();
            let first = lines.next().map(str::trim_end);
            let marker =
                "<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->";
            first == Some(marker)
                || (first
                    == Some(
                        "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->",
                    )
                    && lines.next().map(str::trim_end) == Some(marker))
        }
        _ => false,
    };
    if quota {
        Disposition::QuotaExhausted
    } else {
        Disposition::SubstanceUnknown
    }
}

impl ProviderReviewObservations {
    pub(crate) fn from_gh_value(value: &Value) -> Self {
        let head = string(value, "headRefOid");
        let snapshot_head = if head.len() == 40 && head.bytes().all(|b| b.is_ascii_hexdigit()) {
            head
        } else {
            String::new()
        };
        let mut responses = Vec::new();
        for (field, kind, time) in [
            ("latestReviews", "review", "submittedAt"),
            ("comments", "issue_comment", "createdAt"),
        ] {
            for item in value
                .get(field)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let author = item
                    .get("author")
                    .map(|v| string(v, "login"))
                    .unwrap_or_default();
                if !known_provider(&author) {
                    continue;
                }
                let body = string(item, "body");
                let disposition = classify(&author, &body);
                responses.push(Response {
                    author,
                    source_kind: kind,
                    state: string(item, "state"),
                    timestamp: string(item, time),
                    url: string(item, "url"),
                    body,
                    disposition,
                });
            }
        }
        Self {
            snapshot_head,
            responses,
        }
    }

    fn quota_count(&self) -> usize {
        self.responses
            .iter()
            .filter(|r| r.disposition == Disposition::QuotaExhausted)
            .count()
    }

    pub(crate) fn json_value(&self) -> Value {
        json!({
            "snapshot_head": self.snapshot_head,
            "scope": "observed_responses_only",
            "quota_notice_count": self.quota_count(),
            "responses": self.responses.iter().map(|r| json!({
                "author": r.author, "source_kind": r.source_kind, "state": r.state,
                "timestamp": r.timestamp, "url": r.url, "body": r.body,
                "disposition": r.disposition.as_str(), "subject_attribution": "not_established"
            })).collect::<Vec<_>>()
        })
    }

    pub(crate) fn markdown(&self) -> String {
        let mut text = format!(
            "\n## Observed Provider Responses\n\nQuota notices: {} of {} observed provider responses.\n\n",
            self.quota_count(),
            self.responses.len()
        );
        text.push_str("These response observations are not evidence of review completion or independence. Historical responses are not attributed to the current head. Unknown substance is not a clean review; providers absent from the returned data are not assessed. Original bodies and any returned URLs and timestamps are retained in JSON.\n");
        for r in &self.responses {
            text.push_str(&format!(
                "- {} ({}): `{}`\n",
                r.author,
                r.source_kind,
                r.disposition.as_str()
            ));
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_anchored_provider_notices_are_quota() {
        let codex = "You have reached your Codex usage limits for code reviews.";
        for author in ["chatgpt-codex-connector", "chatgpt-codex-connector[bot]"] {
            assert_eq!(classify(author, codex), Disposition::QuotaExhausted);
        }
        for (author, body) in [
            ("someone", codex.to_owned()),
            ("fake-chatgpt-codex-connector[bot]", codex.to_owned()),
            ("chatgpt-codex-connector[bot][bot]", codex.to_owned()),
            ("chatgpt-codex-connector[bot]", format!("Reviewed the quota guard. The error message is '{codex}' Add a boundary test.")),
            ("chatgpt-codex-connector[bot]", format!("```\n{codex}\n```")),
            ("chatgpt-codex-connector[bot]", format!("Previous automated response:\n> {codex}\n\nThe code handles this correctly.")),
            ("chatgpt-codex-connector[bot]", "LGTM".to_owned()),
            ("chatgpt-codex-connector[bot]", String::new()),
            ("sourcery-ai[bot]", "Reviewed the weekly rate limit of 500000 diff characters.".to_owned()),
            ("sourcery-ai[bot]", "Sorry @user, you have reached your weekly rate limit of unlimited diff characters.".to_owned()),
            ("coderabbitai[bot]", "Review details: no defects found.\n<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->".to_owned()),
            ("gemini-code-assist[bot]", "> Quoted failure:\n> You have reached your daily quota limit.".to_owned()),
        ] {
            assert_eq!(classify(author, &body), Disposition::SubstanceUnknown, "{author}: {body}");
        }
    }

    #[test]
    fn indented_code_examples_are_not_provider_notices() {
        for (author, notice) in [
            (
                "chatgpt-codex-connector[bot]",
                "You have reached your Codex usage limits for code reviews.",
            ),
            (
                "gemini-code-assist[bot]",
                "> [!WARNING]\n> You have reached your daily quota limit.",
            ),
            (
                "sourcery-ai[bot]",
                "Sorry @person, you have reached your weekly rate limit of 500000 diff characters.",
            ),
            (
                "coderabbitai[bot]",
                "<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->",
            ),
        ] {
            assert_eq!(classify(author, notice), Disposition::QuotaExhausted);
            for indent in ["    ", "\t"] {
                let example = notice
                    .lines()
                    .map(|line| format!("{indent}{line}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                assert_eq!(
                    classify(author, &example),
                    Disposition::SubstanceUnknown,
                    "{author}: {example}"
                );
            }
        }
    }

    #[test]
    fn native_states_do_not_establish_substance_or_erase_quota() {
        for state in [
            "APPROVED",
            "COMMENTED",
            "CHANGES_REQUESTED",
            "DISMISSED",
            "PENDING",
        ] {
            let value = json!({"latestReviews": [
                {"author": {"login":"chatgpt-codex-connector[bot]"}, "state":state, "body":"You have reached your Codex usage limits for code reviews."},
                {"author": {"login":"chatgpt-codex-connector[bot]"}, "state":state, "body":"LGTM"}
            ]});
            let result = ProviderReviewObservations::from_gh_value(&value).json_value();
            assert_eq!(result["responses"][0]["state"], state);
            assert_eq!(result["responses"][0]["disposition"], "quota_exhausted");
            assert_eq!(result["responses"][1]["disposition"], "substance_unknown");
            assert_eq!(result["quota_notice_count"], 1);
        }
    }

    #[test]
    fn historical_notice_and_later_review_remain_separate_observations() {
        let raw = "You have reached your Codex usage limits for code reviews.\r\n`code` \"quoted\" — 日本語\n";
        let value = json!({"headRefOid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "latestReviews":[
            {"author":{"login":"chatgpt-codex-connector[bot]"},"state":"APPROVED","body":"Reviewed the fix; no findings.","submittedAt":"2026-10-14T05:00:00Z"}
        ], "comments":[
            {"author":{"login":"chatgpt-codex-connector[bot]"},"body":raw,"createdAt":"2026-10-13T05:00:00Z"}
        ]});
        let observation = ProviderReviewObservations::from_gh_value(&value);
        let result = observation.json_value();
        assert_eq!(result["responses"][0]["disposition"], "substance_unknown");
        assert_eq!(result["responses"][1]["body"], raw);
        assert_eq!(result["responses"][1]["disposition"], "quota_exhausted");
        assert_eq!(
            result["responses"][1]["subject_attribution"],
            "not_established"
        );
        assert_eq!(result["scope"], "observed_responses_only");
        assert!(
            observation
                .markdown()
                .contains("Quota notices: 1 of 2 observed provider responses")
        );
        assert!(!observation.markdown().contains("completed_no_findings"));
    }

    #[test]
    fn missing_or_malformed_data_does_not_manufacture_review() {
        for value in [
            json!({}),
            json!({"headRefOid":"short", "latestReviews":null, "comments":{}}),
        ] {
            let result = ProviderReviewObservations::from_gh_value(&value).json_value();
            assert_eq!(result["snapshot_head"], "");
            assert_eq!(result["quota_notice_count"], 0);
            assert_eq!(result["responses"], json!([]));
        }
    }
}
