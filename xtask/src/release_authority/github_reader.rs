//! Bounded, read-only GitHub record reader (ripr#1651).
//!
//! The trait has three read operations and no write operation. Every failure
//! to obtain a complete observation is `Unavailable`: an HTTP 403 and a 404
//! cannot be told apart from outside, so neither is treated as proof that a
//! record does not exist.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

/// Pages of 100 issue comments read before the index is declared unbounded.
pub(crate) const MAX_INDEX_PAGES: usize = 10;
const PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepositoryObservation {
    pub(crate) id: u64,
    pub(crate) full_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentObservation {
    pub(crate) id: u64,
    pub(crate) issue: u64,
    pub(crate) author_id: u64,
    pub(crate) author_login: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Unavailable {
    pub(crate) kind: &'static str,
    pub(crate) detail: String,
}

impl Unavailable {
    pub(crate) fn new(kind: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

pub(crate) trait RecordReader {
    /// `github_api` for a live read; anything else is a replay, not authority.
    fn source(&self) -> &'static str;
    fn repository(&self, name: &str) -> Result<RepositoryObservation, Unavailable>;
    fn comment(&self, repository: &str, comment_id: u64)
    -> Result<CommentObservation, Unavailable>;
    /// Every comment on the issue, or `Unavailable` when the listing is
    /// incomplete or longer than the bounded index.
    fn issue_comments(
        &self,
        repository: &str,
        issue: u64,
    ) -> Result<Vec<CommentObservation>, Unavailable>;
}

pub(crate) fn valid_repository_name(name: &str) -> bool {
    let part = |text: &str| {
        !text.is_empty()
            && text != "."
            && text != ".."
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    matches!(name.split('/').collect::<Vec<_>>().as_slice(), [owner, repo] if part(owner) && part(repo))
}

fn parse_json(bytes: &[u8]) -> Result<Value, Unavailable> {
    serde_json::from_slice(bytes)
        .map_err(|e| Unavailable::new("malformed_response", format!("response is not JSON: {e}")))
}

fn parse_repository(value: &Value) -> Result<RepositoryObservation, Unavailable> {
    match (
        value.get("id").and_then(Value::as_u64),
        value.get("full_name").and_then(Value::as_str),
    ) {
        (Some(id), Some(full_name)) => Ok(RepositoryObservation {
            id,
            full_name: full_name.to_string(),
        }),
        _ => Err(Unavailable::new(
            "malformed_response",
            "repository response lacks id or full_name",
        )),
    }
}

fn parse_comment(value: &Value, repository: &str) -> Result<CommentObservation, Unavailable> {
    let incomplete =
        |what: &str| Unavailable::new("incomplete_observation", format!("comment lacks {what}"));
    let text = |pointer: &str, what: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| incomplete(what))
    };
    let issue_url = text("/issue_url", "issue_url")?;
    // The location is taken only from an issue URL of this exact repository.
    let issue = issue_url
        .rsplit_once("/issues/")
        .filter(|(prefix, _)| {
            prefix
                .rsplit_once("/repos/")
                .is_some_and(|(_, name)| name == repository)
        })
        .and_then(|(_, number)| super::parse_plain_u64(number))
        .ok_or_else(|| incomplete("an issue location in the requested repository"))?;
    Ok(CommentObservation {
        id: value
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| incomplete("id"))?,
        issue,
        author_id: value
            .pointer("/user/id")
            .and_then(Value::as_u64)
            .ok_or_else(|| incomplete("author id"))?,
        author_login: text("/user/login", "author login")?,
        created_at: text("/created_at", "created_at")?,
        updated_at: text("/updated_at", "updated_at")?,
        body: text("/body", "body")?,
    })
}

fn parse_comment_page(
    value: &Value,
    repository: &str,
) -> Result<Vec<CommentObservation>, Unavailable> {
    value
        .as_array()
        .ok_or_else(|| Unavailable::new("malformed_response", "comment listing is not an array"))?
        .iter()
        .map(|item| parse_comment(item, repository))
        .collect()
}

/// The complete argument list for one read. `-X GET` is explicit so no field
/// flag can turn the request into a write, and the path set is closed.
pub(crate) fn live_argv(path: &str) -> Vec<String> {
    [
        "api",
        "-X",
        "GET",
        "--hostname",
        "github.com",
        "-H",
        "Accept: application/vnd.github+json",
        path,
    ]
    .iter()
    .map(|part| (*part).to_string())
    .collect()
}

fn classify_failure(stderr: &str) -> &'static str {
    let status = stderr
        .split("HTTP ")
        .nth(1)
        .and_then(|rest| rest.get(..3))
        .and_then(|code| code.parse::<u16>().ok());
    match status {
        Some(403) => "http_403",
        Some(404) => "http_404_or_inaccessible",
        Some(429) => "http_429",
        Some(500..=599) => "http_5xx",
        Some(_) => "http_other",
        None => "transport_or_tls",
    }
}

pub(crate) struct LiveReader;

impl LiveReader {
    pub(crate) fn new() -> Self {
        Self
    }

    fn get(&self, path: &str) -> Result<Value, Unavailable> {
        let output = crate::run::capture_bytes_in_dir_with_budget(
            Path::new("gh"),
            &live_argv(path),
            (Path::new("."), None),
            &[],
            crate::run::ByteCaptureBudget {
                timeout: Duration::from_mins(1),
                stdout_bytes: 8 * 1024 * 1024,
                stderr_bytes: 64 * 1024,
            },
            "release-authority read",
        )
        .map_err(|e| Unavailable::new("reader_failed", e))?;
        if output.timed_out {
            return Err(Unavailable::new("timeout", format!("GET {path} timed out")));
        }
        if !output.status.is_some_and(|status| status.success()) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Unavailable::new(
                classify_failure(&stderr),
                format!("GET {path} failed"),
            ));
        }
        parse_json(&output.stdout)
    }
}

impl RecordReader for LiveReader {
    fn source(&self) -> &'static str {
        "github_api"
    }

    fn repository(&self, name: &str) -> Result<RepositoryObservation, Unavailable> {
        parse_repository(&self.get(&format!("repos/{name}"))?)
    }

    fn comment(
        &self,
        repository: &str,
        comment_id: u64,
    ) -> Result<CommentObservation, Unavailable> {
        let value = self.get(&format!("repos/{repository}/issues/comments/{comment_id}"))?;
        parse_comment(&value, repository)
    }

    fn issue_comments(
        &self,
        repository: &str,
        issue: u64,
    ) -> Result<Vec<CommentObservation>, Unavailable> {
        let read_page = |page: usize| {
            let value = self.get(&format!(
                "repos/{repository}/issues/{issue}/comments?per_page={PAGE_SIZE}&page={page}"
            ))?;
            parse_comment_page(&value, repository)
        };
        stable_listing(issue, read_page)
    }
}

/// One complete listing: every page up to the bound, ended by a short page.
///
/// An index of exactly the bound ends with a full page, so one further page is
/// read to tell "exactly the bound" (empty) from "more than the bound".
fn read_listing(
    issue: u64,
    read_page: &mut dyn FnMut(usize) -> Result<Vec<CommentObservation>, Unavailable>,
) -> Result<Vec<CommentObservation>, Unavailable> {
    let mut all = Vec::new();
    for page in 1..=MAX_INDEX_PAGES + 1 {
        let comments = read_page(page)?;
        if page > MAX_INDEX_PAGES && !comments.is_empty() {
            break;
        }
        let last = comments.len() < PAGE_SIZE;
        all.extend(comments);
        if last {
            return Ok(all);
        }
    }
    Err(Unavailable::new(
        "unexpected_pagination",
        format!("issue {issue} has more comments than the bounded index reads"),
    ))
}

/// Offset pagination shifts when a comment is created or deleted between page
/// requests, which can hide a record from one pass. The listing is therefore
/// read twice and accepted only when both passes are identical.
fn stable_listing(
    issue: u64,
    mut read_page: impl FnMut(usize) -> Result<Vec<CommentObservation>, Unavailable>,
) -> Result<Vec<CommentObservation>, Unavailable> {
    let first = read_listing(issue, &mut read_page)?;
    let second = read_listing(issue, &mut read_page)?;
    if first == second {
        Ok(first)
    } else {
        Err(Unavailable::new(
            "inconsistent_index",
            format!("issue {issue} comments changed while the index was being read"),
        ))
    }
}

/// Replays saved API responses: `repository.json`, `comment-<id>.json` and
/// `issue-<number>-comments.json`. A missing or oversized file is unavailable.
pub(crate) struct FixtureReader {
    root: PathBuf,
}

impl FixtureReader {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn read(&self, name: &str) -> Result<Value, Unavailable> {
        let path = self.root.join(name);
        let unavailable = |detail: String| Unavailable::new("fixture_unavailable", detail);
        let info = std::fs::metadata(&path).map_err(|e| unavailable(format!("{name}: {e}")))?;
        if !info.is_file() || info.len() > 8 * 1024 * 1024 {
            return Err(unavailable(format!(
                "{name}: not a regular file under 8 MiB"
            )));
        }
        parse_json(&std::fs::read(&path).map_err(|e| unavailable(format!("{name}: {e}")))?)
    }
}

impl RecordReader for FixtureReader {
    fn source(&self) -> &'static str {
        "saved_fixture"
    }

    fn repository(&self, _name: &str) -> Result<RepositoryObservation, Unavailable> {
        parse_repository(&self.read("repository.json")?)
    }

    fn comment(
        &self,
        repository: &str,
        comment_id: u64,
    ) -> Result<CommentObservation, Unavailable> {
        parse_comment(
            &self.read(&format!("comment-{comment_id}.json"))?,
            repository,
        )
    }

    fn issue_comments(
        &self,
        repository: &str,
        issue: u64,
    ) -> Result<Vec<CommentObservation>, Unavailable> {
        let comments = parse_comment_page(
            &self.read(&format!("issue-{issue}-comments.json"))?,
            repository,
        )?;
        if comments.len() > MAX_INDEX_PAGES * PAGE_SIZE {
            return Err(Unavailable::new(
                "unexpected_pagination",
                "saved listing is longer than the bounded index",
            ));
        }
        Ok(comments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn comment(issue_url: &str) -> Value {
        json!({
            "id": 7, "issue_url": issue_url, "body": "text",
            "created_at": "2026-11-21T00:00:00Z", "updated_at": "2026-11-21T00:00:00Z",
            "user": {"id": 9, "login": "someone"},
        })
    }

    #[test]
    fn release_authorization_record_reader_only_issues_closed_get_requests() -> Result<(), String> {
        let argv = live_argv("repos/EffortlessMetrics/ripr/issues/comments/7");
        if argv.first().map(String::as_str) != Some("api")
            || argv.get(1..3) != Some(&["-X".to_string(), "GET".to_string()][..])
        {
            return Err(format!("a read was not an explicit GET: {argv:?}"));
        }
        let write_flags = [
            "-f",
            "-F",
            "--field",
            "--raw-field",
            "--input",
            "--method",
            "POST",
            "PATCH",
            "PUT",
            "DELETE",
        ];
        if argv.iter().any(|part| write_flags.contains(&part.as_str())) {
            return Err(format!("a read carries a write-capable argument: {argv:?}"));
        }
        for (name, valid) in [
            ("EffortlessMetrics/ripr", true),
            ("EffortlessMetrics/ripr/issues", false),
            ("../ripr", false),
            ("EffortlessMetrics/ripr?x=1", false),
            ("EffortlessMetrics/", false),
        ] {
            if valid_repository_name(name) != valid {
                return Err(format!("repository name `{name}` validity is not {valid}"));
            }
        }
        Ok(())
    }

    #[test]
    fn release_authorization_record_reader_takes_location_from_the_requested_repository()
    -> Result<(), String> {
        let repository = "EffortlessMetrics/ripr";
        let here = "https://api.github.com/repos/EffortlessMetrics/ripr/issues/1470";
        if parse_comment(&comment(here), repository).map(|seen| seen.issue) != Ok(1470) {
            return Err("an exact issue location was not read".to_string());
        }
        for elsewhere in [
            "https://api.github.com/repos/EffortlessMetrics/ripr-swarm/issues/1470",
            "https://api.github.com/repos/EffortlessMetrics/ripr/pulls/1470",
            "https://api.github.com/repos/EffortlessMetrics/ripr/issues/1470/x",
        ] {
            if parse_comment(&comment(elsewhere), repository).is_ok() {
                return Err(format!("location `{elsewhere}` was accepted"));
            }
        }
        let mut without_author = comment(here);
        without_author["user"] = Value::Null;
        match parse_comment(&without_author, repository) {
            Err(unavailable) if unavailable.kind == "incomplete_observation" => {}
            other => return Err(format!("missing author observation gave {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn release_authorization_record_reader_keeps_every_http_failure_unavailable()
    -> Result<(), String> {
        for (stderr, kind) in [
            ("gh: Not Found (HTTP 404)", "http_404_or_inaccessible"),
            ("gh: Forbidden (HTTP 403)", "http_403"),
            ("gh: rate limited (HTTP 429)", "http_429"),
            ("gh: Bad Gateway (HTTP 502)", "http_5xx"),
            ("tls: failed to verify certificate", "transport_or_tls"),
        ] {
            if classify_failure(stderr) != kind {
                return Err(format!(
                    "`{stderr}` classified as {}",
                    classify_failure(stderr)
                ));
            }
        }
        Ok(())
    }

    fn numbered(count: usize) -> Vec<CommentObservation> {
        (1..=count as u64)
            .map(|id| CommentObservation {
                id,
                issue: 5,
                author_id: 9,
                author_login: "someone".to_string(),
                created_at: "2026-11-21T00:00:00Z".to_string(),
                updated_at: "2026-11-21T00:00:00Z".to_string(),
                body: String::new(),
            })
            .collect()
    }

    fn page_of(all: &[CommentObservation], page: usize) -> Vec<CommentObservation> {
        all.chunks(PAGE_SIZE)
            .nth(page - 1)
            .map(<[CommentObservation]>::to_vec)
            .unwrap_or_default()
    }

    #[test]
    fn release_authorization_record_reader_index_bound_is_inclusive() -> Result<(), String> {
        let bound = MAX_INDEX_PAGES * PAGE_SIZE;
        for (count, complete) in [
            (0, true),
            (PAGE_SIZE - 1, true),
            (PAGE_SIZE, true),
            (bound - 1, true),
            (bound, true),
            (bound + 1, false),
            (bound + PAGE_SIZE, false),
        ] {
            let all = numbered(count);
            match stable_listing(5, |page| Ok(page_of(&all, page))) {
                Ok(read) if complete && read == all => {}
                Err(unavailable) if !complete && unavailable.kind == "unexpected_pagination" => {}
                other => {
                    return Err(format!(
                        "{count} comments gave {:?}",
                        other.map(|read| read.len())
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn release_authorization_record_reader_refuses_an_index_that_shifts_mid_read()
    -> Result<(), String> {
        // Comment 1 is deleted after page 1 is served: comment 101, which could
        // be a revocation, moves onto page 1 and is in no page of that pass.
        let before = numbered(PAGE_SIZE + 50);
        let after = before[1..].to_vec();
        let mut requests = 0;
        let shifted = stable_listing(5, |page| {
            requests += 1;
            Ok(page_of(if requests == 1 { &before } else { &after }, page))
        });
        match shifted {
            Err(unavailable) if unavailable.kind == "inconsistent_index" => {}
            other => {
                return Err(format!(
                    "a listing that lost comment {} mid-read gave {:?}",
                    PAGE_SIZE + 1,
                    other.map(|read| read.len())
                ));
            }
        }
        // A failing page is unavailable, never a shorter index.
        let failed = stable_listing(5, |page| {
            if page == 2 {
                Err(Unavailable::new("http_5xx", "page 2"))
            } else {
                Ok(page_of(&before, page))
            }
        });
        match failed {
            Err(unavailable) if unavailable.kind == "http_5xx" => Ok(()),
            other => Err(format!("a failed page gave {:?}", other.map(|r| r.len()))),
        }
    }
}
