//! A maintainer-selected, advisory comment for one freshly compared queue entry.
use super::{Outcome, REPO, Row, Sweep, with_objects};
use crate::reports::source_promotion::validate_sha;
use serde::Serialize;
use serde_json::{Value, json};
use std::fmt::Write;
use std::fs;
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
        let bytes = self.checked(
            "gh",
            &[
                "api",
                "--hostname",
                "github.com",
                "--method",
                "POST",
                &endpoint,
                "--input",
                "-",
            ],
            Some(&payload),
        )?;
        let published: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        published["id"].as_u64().filter(|id| *id > 0).ok_or(
            "comment submission returned no positive ID; inspect GitHub before retrying".into(),
        )
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Unproved,
    AlreadyPosted,
    Preview,
    Posted(u64),
}

fn deliver(
    api: &impl NoticeApi,
    subject: &Subject<'_>,
    initial: &Member,
    body: Option<&str>,
    post: bool,
) -> Result<Disposition, String> {
    let Some(body) = body else {
        return Ok(Disposition::Unproved);
    };
    if api.seen(subject, &initial.viewer)? {
        return Ok(Disposition::AlreadyPosted);
    }
    let last = api.live(subject)?;
    if last.entry != initial.entry || last.viewer != initial.viewer {
        return Err(
            "native queue entry or authenticated viewer changed during verification".into(),
        );
    }
    if post {
        api.post(subject, body).map(Disposition::Posted)
    } else {
        Ok(Disposition::Preview)
    }
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
        let body = advisory(&subject, &row);
        let path = out.join("queue-conflict-notice.json");
        let mut receipt = json!({"schema_version":1,"repository":REPO,"main":main,"entry":initial.entry,"row":row,"body":body,"post_requested":post,"disposition":"pending"});
        let write = |record: &Value| {
            fs::write(
                &path,
                serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        };
        write(&receipt)?;
        match deliver(&sweep, &subject, &initial, body.as_deref(), post) {
            Ok(done) => receipt["disposition"] = json!(done),
            Err(error) => {
                receipt["disposition"] = json!("unconfirmed");
                receipt["error"] = json!(error);
                write(&receipt)?;
                return Err(error);
            }
        }
        write(&receipt)?;
        println!("{}: {}", path.display(), receipt["disposition"]);
        Ok(())
    })
}

#[cfg(test)]
mod tests;
