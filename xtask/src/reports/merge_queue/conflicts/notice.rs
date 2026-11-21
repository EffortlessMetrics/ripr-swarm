//! A maintainer-selected, advisory comment for one freshly compared queue entry.
use super::{Outcome, REPO, Row, Sweep, with_objects};
use crate::reports::source_promotion::validate_sha;
use serde::Serialize;
use serde_json::{Value, json};
use std::fmt::Write;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

const USAGE: &str =
    "cargo xtask merge-queue conflict-notice <new-output-directory> <pr> <main> <head> [--post]";
const LIVE: &str = "query=query($pr:Int!){viewer{login} repository(owner:\"EffortlessMetrics\",name:\"ripr-swarm\"){nameWithOwner ref(qualifiedName:\"refs/heads/main\"){target{oid}} pullRequest(number:$pr){number state isDraft baseRefName headRefOid mergeQueueEntry{id position}}}}";

#[derive(PartialEq, Eq)]
struct Member {
    entry: String,
    position: usize,
    viewer: String,
}

struct Subject<'a> {
    pr: u64,
    main: &'a str,
    head: &'a str,
}

impl Subject<'_> {
    fn member(&self, value: &Value) -> Result<Member, String> {
        let repo = &value["data"]["repository"];
        let pr = &repo["pullRequest"];
        let entry = &pr["mergeQueueEntry"];
        if value.get("errors").is_some()
            || repo["nameWithOwner"] != REPO
            || repo["ref"]["target"]["oid"] != self.main
            || pr["headRefOid"] != self.head
            || pr["number"].as_u64() != Some(self.pr)
            || pr["baseRefName"] != "main"
            || pr["state"] != "OPEN"
            || pr["isDraft"] != false
        {
            return Err("the selected repository, base, open PR or head no longer matches".into());
        }
        Ok(Member {
            entry: entry["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("PR has no current native queue entry")?
                .into(),
            position: entry["position"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or("unreadable queue position")?,
            viewer: value["data"]["viewer"]["login"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("missing authenticated GitHub viewer")?
                .into(),
        })
    }

    fn marker(&self) -> String {
        format!(
            "<!-- ripr-queue-conflict:v1 {} {} {} -->",
            self.pr, self.main, self.head
        )
    }
}

fn already_posted(pages: &Value, marker: &str, viewer: &str) -> Result<bool, String> {
    let pages = pages
        .as_array()
        .filter(|p| !p.is_empty())
        .ok_or("unreadable paginated GitHub comments")?;
    for page in pages {
        for comment in page.as_array().ok_or("unreadable issue comment page")? {
            let body = comment["body"]
                .as_str()
                .ok_or("unreadable issue comment body")?;
            let login = comment["user"]["login"]
                .as_str()
                .ok_or("unreadable issue comment author")?;
            let trusted = login == viewer
                || matches!(
                    comment["author_association"].as_str(),
                    Some("MEMBER" | "OWNER" | "COLLABORATOR")
                );
            if trusted && body.lines().next() == Some(marker) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn quoted_path(path: &str) -> String {
    // Git paths may contain newlines, Markdown or mentions. Only the safe
    // ASCII path spelling is displayed literally inside the inline code span.
    path.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/._-".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn advisory(subject: &Subject<'_>, row: &Row) -> Option<String> {
    if row.pr != subject.pr || row.head != subject.head {
        return None;
    }
    let Outcome::Conflict { paths, .. } = &row.result else {
        return None;
    };
    if paths.is_empty() {
        return None;
    }
    let mut body = format!(
        "{}\nA test merge of PR #{} at `{}` against observed main `{}` found these conflicts. Each transition below is the first-parent edge where that path became conflicted.\n",
        subject.marker(),
        subject.pr,
        subject.head,
        subject.main
    );
    for (path, boundary) in paths {
        let transition = boundary.as_ref()?;
        if path.is_empty() {
            return None;
        }
        writeln!(
            &mut body,
            "\n- `{}` between `{}` and `{}`",
            quoted_path(path),
            transition.first_parent,
            transition.introduced_at
        )
        .ok()?;
        if body.len() > 55_000 {
            return None;
        }
    }
    body.push_str("\nCheck the current base and PR head before resolving these paths. This observation concerns the two commits above.");
    Some(body)
}

trait NoticeApi {
    fn live(&self, subject: &Subject<'_>) -> Result<Member, String>;
    fn seen(&self, subject: &Subject<'_>, viewer: &str) -> Result<bool, String>;
    fn post(&self, subject: &Subject<'_>, body: &str) -> Result<u64, String>;
}

impl NoticeApi for Sweep<'_> {
    fn live(&self, subject: &Subject<'_>) -> Result<Member, String> {
        let bytes = self.checked(
            "gh",
            &[
                "api",
                "--hostname",
                "github.com",
                "graphql",
                "-f",
                LIVE,
                "-F",
                &format!("pr={}", subject.pr),
            ],
            None,
        )?;
        subject.member(&serde_json::from_slice(&bytes).map_err(|e| e.to_string())?)
    }
    fn seen(&self, subject: &Subject<'_>, viewer: &str) -> Result<bool, String> {
        let endpoint = format!("repos/{REPO}/issues/{}/comments?per_page=100", subject.pr);
        let bytes = self.checked(
            "gh",
            &[
                "api",
                "--hostname",
                "github.com",
                "--method",
                "GET",
                "--paginate",
                "--slurp",
                &endpoint,
            ],
            None,
        )?;
        already_posted(
            &serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            &subject.marker(),
            viewer,
        )
    }
    fn post(&self, subject: &Subject<'_>, body: &str) -> Result<u64, String> {
        let endpoint = format!("repos/{REPO}/issues/{}/comments", subject.pr);
        let payload = serde_json::to_vec(&json!({"body":body})).map_err(|e| e.to_string())?;
        let (exit, bytes) = self.command(
            "gh",
            &[
                "api",
                "--hostname",
                "github.com",
                "--include",
                "--method",
                "POST",
                &endpoint,
                "--input",
                "-",
            ],
            Some(&payload),
        )?;
        posted_comment_id(exit, &bytes)
    }
}

// gh api --include emits a status line followed by headers and a JSON body.
// Retain only the numeric HTTP status on failure; response headers and bodies
// may contain sensitive details.
fn posted_comment_id(exit: i32, bytes: &[u8]) -> Result<u64, String> {
    let first = bytes.split(|b| *b == b'\n').next().unwrap_or_default();
    let line = std::str::from_utf8(first).unwrap_or_default();
    let mut fields = line.split_ascii_whitespace();
    let status = fields
        .next()
        .filter(|version| version.starts_with("HTTP/"))
        .and_then(|_| fields.next())
        .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|s| (100..600).contains(s))
        .ok_or_else(|| format!("gh api exited {exit} without a readable HTTP status"))?;
    if exit != 0 || status != 201 {
        return Err(format!(
            "GitHub comment request returned HTTP {status} (gh exit {exit})"
        ));
    }
    let response = std::str::from_utf8(bytes)
        .map_err(|_| "GitHub returned HTTP 201 with an unreadable response")?;
    let (_, body) = response
        .split_once("\r\n\r\n")
        .or_else(|| response.split_once("\n\n"))
        .ok_or("GitHub returned HTTP 201 without a response body")?;
    let published: Value = serde_json::from_str(body)
        .map_err(|_| "GitHub returned HTTP 201 with an unreadable comment body")?;
    published["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or("GitHub returned HTTP 201 without a positive comment ID".into())
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Unproved,
    AlreadyPosted,
    Preview,
    Posted(u64),
}

enum DeliveryFailure {
    NotSubmitted(String),
    Unconfirmed(String),
}

impl DeliveryFailure {
    fn status(&self) -> &'static str {
        match self {
            Self::NotSubmitted(_) => "not_submitted",
            Self::Unconfirmed(_) => "unconfirmed",
        }
    }
}

impl From<String> for DeliveryFailure {
    fn from(error: String) -> Self {
        Self::NotSubmitted(error)
    }
}

impl From<DeliveryFailure> for String {
    fn from(error: DeliveryFailure) -> Self {
        match error {
            DeliveryFailure::NotSubmitted(cause) => format!("comment not submitted: {cause}"),
            DeliveryFailure::Unconfirmed(cause) => format!(
                "submission unconfirmed: {cause}; inspect GitHub and the pending intent before considering another post"
            ),
        }
    }
}

fn deliver(
    api: &impl NoticeApi,
    subject: &Subject<'_>,
    initial: &Member,
    body: Option<&str>,
    post: bool,
) -> Result<Disposition, DeliveryFailure> {
    let Some(body) = body else {
        return Ok(Disposition::Unproved);
    };
    if api.seen(subject, &initial.viewer)? {
        return Ok(Disposition::AlreadyPosted);
    }
    let last = api.live(subject)?;
    if last.entry != initial.entry || last.viewer != initial.viewer {
        return Err(DeliveryFailure::NotSubmitted(
            "native queue entry or authenticated viewer changed during verification".into(),
        ));
    }
    if post {
        api.post(subject, body)
            .map(Disposition::Posted)
            .map_err(DeliveryFailure::Unconfirmed)
    } else {
        Ok(Disposition::Preview)
    }
}

// with_objects gives each attempt an exclusive new directory. Only complete
// records acquire their public filename. A terminal write never opens the intent.
fn commit_record(path: &Path, record: &Value) -> Result<(), String> {
    let stage = path.with_extension("partial");
    let bytes = serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?;
    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create_new(&stage)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&stage, path)
    };
    write().map_err(|e| format!("could not commit {}: {e}", path.display()))
}

fn record_delivery(
    out: &Path,
    api: &impl NoticeApi,
    subject: &Subject<'_>,
    initial: &Member,
    row: &Row,
    post: bool,
) -> Result<(), String> {
    let body = advisory(subject, row);
    let mut receipt = json!({"schema_version":1,"repository":REPO,"main":subject.main,"entry":initial.entry,"row":row,"body":body,"post_requested":post,"disposition":"pending"});
    let intent = out.join("queue-conflict-notice-pending.json");
    commit_record(&intent, &receipt).map_err(|e| format!("comment not submitted: {e}"))?;
    let outcome = match deliver(api, subject, initial, body.as_deref(), post) {
        Ok(done) => {
            receipt["disposition"] = json!(done);
            Ok(())
        }
        Err(failure) => {
            receipt["disposition"] = json!(failure.status());
            let message = String::from(failure);
            receipt["error"] = json!(message);
            Err(message)
        }
    };
    let terminal = out.join("queue-conflict-notice.json");
    if let Err(local) = commit_record(&terminal, &receipt) {
        let delivery = match outcome {
            Ok(()) => format!("delivery outcome {}", receipt["disposition"]),
            Err(cause) => cause,
        };
        return Err(format!(
            "{delivery}; {local}; pending intent: {}",
            intent.display()
        ));
    }
    println!("{}: {}", terminal.display(), receipt["disposition"]);
    outcome
}

pub(in crate::reports::merge_queue) fn run(args: &[String]) -> Result<(), String> {
    if matches!(args,[flag] if flag=="--help" || flag=="-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let (args, post) = match args.split_last() {
        Some((flag, rest)) if flag == "--post" => (rest, true),
        _ => (args, false),
    };
    let [out, pr, main, head] = args else {
        return Err(USAGE.into());
    };
    if post && std::env::var_os("CI").is_some() {
        return Err(
            "posting a queue conflict notice requires an explicit local operator invocation".into(),
        );
    }
    let pr: u64 = pr
        .parse()
        .ok()
        .filter(|n| *n > 0 && *n <= i32::MAX as u64)
        .ok_or(USAGE)?;
    validate_sha("main", "commit", main)?;
    validate_sha("head", "commit", head)?;
    let subject = Subject { pr, main, head };
    let out = Path::new(out);
    with_objects(out, |objects| {
        let sweep = Sweep {
            objects,
            deadline: Instant::now() + Duration::from_mins(8),
        };
        let initial = sweep.live(&subject)?;
        let mut row = Row {
            position: initial.position,
            pr,
            head: head.clone(),
            result: Outcome::Unexamined {
                reason: "pending comparison".into(),
            },
        };
        sweep.prepare(
            main,
            std::slice::from_ref(&row),
            "git@github.com:EffortlessMetrics/ripr-swarm.git",
        )?;
        sweep.classify(main, std::slice::from_mut(&mut row));
        let _history = sweep.attribute_history(main, std::slice::from_mut(&mut row));
        record_delivery(out, &sweep, &subject, &initial, &row, post)
    })
}

#[cfg(test)]
mod tests;
