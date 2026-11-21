use super::super::Boundary;
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

const MAIN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn subject() -> Subject<'static> {
    Subject {
        pr: 37,
        main: MAIN,
        head: HEAD,
    }
}
fn membership() -> Value {
    json!({"data":{"viewer":{"login":"maintainer"},"repository":{"nameWithOwner":REPO,"ref":{"target":{"oid":MAIN}},"pullRequest":{"number":37,"state":"OPEN","isDraft":false,"baseRefName":"main","headRefOid":HEAD,"mergeQueueEntry":{"id":"first","position":10}}}}})
}

#[test]
fn membership_requires_exact_open_queued_subject() -> Result<(), String> {
    let subject = subject();
    let valid = membership();
    subject.member(&valid)?;
    for (path, wrong) in [
        ("/data/repository/nameWithOwner", json!("other/repo")),
        ("/data/repository/ref/target/oid", json!(HEAD)),
        ("/data/repository/pullRequest/headRefOid", json!(MAIN)),
        ("/data/repository/pullRequest/number", json!(38)),
        ("/data/repository/pullRequest/baseRefName", json!("other")),
        ("/data/repository/pullRequest/state", json!("CLOSED")),
        ("/data/repository/pullRequest/isDraft", json!(true)),
        ("/data/repository/pullRequest/mergeQueueEntry", Value::Null),
    ] {
        let mut input = valid.clone();
        *input.pointer_mut(path).ok_or("missing fixture pointer")? = wrong;
        if subject.member(&input).is_ok() {
            return Err(format!("admitted stale subject: {path}"));
        }
    }
    let mut partial = valid;
    partial["errors"] = json!([{"message":"partial API read"}]);
    if subject.member(&partial).is_ok() {
        return Err("accepted partial read".into());
    }
    Ok(())
}

fn row(result: Outcome) -> Row {
    Row {
        position: 10,
        pr: 37,
        head: HEAD.into(),
        result,
    }
}
fn conflict() -> Row {
    row(Outcome::Conflict {
        paths: BTreeMap::from([(
            "src/odd`\n@name.rs".into(),
            Some(Boundary {
                first_parent: HEAD.into(),
                introduced_at: MAIN.into(),
            }),
        )]),
        unattributed_reason: None,
    })
}

#[test]
fn only_named_attributed_git_conflicts_can_be_presented() -> Result<(), String> {
    let target = subject();
    let body = advisory(&target, &conflict()).ok_or("missing genuine conflict")?;
    if !body.starts_with(&target.marker())
        || !body.contains("src/odd%60%0A%40name.rs")
        || !body.contains(MAIN)
        || !body.contains(HEAD)
    {
        return Err("missing identity or unsafe path formatting".into());
    }
    for other in [
        Outcome::Clean,
        Outcome::Unexamined {
            reason: "timeout".into(),
        },
        Outcome::Conflict {
            paths: BTreeMap::new(),
            unattributed_reason: Some("no names".into()),
        },
        Outcome::Conflict {
            paths: BTreeMap::from([("src/lib.rs".into(), None)]),
            unattributed_reason: None,
        },
    ] {
        if advisory(&target, &row(other)).is_some() {
            return Err("prepared notice without named, attributed conflict".into());
        }
    }
    let mut other = conflict();
    other.head = MAIN.into();
    if advisory(&target, &other).is_some() {
        return Err("selected a different PR head".into());
    }
    Ok(())
}

struct FakeApi {
    state: RefCell<Value>,
    comments: RefCell<Value>,
    writes: Cell<u64>,
    fail_read: Cell<bool>,
    lose_acknowledgement: bool,
}
impl FakeApi {
    fn new(lose_acknowledgement: bool) -> Self {
        Self {
            state: RefCell::new(membership()),
            comments: RefCell::new(json!([[]])),
            writes: Cell::new(0),
            fail_read: Cell::new(false),
            lose_acknowledgement,
        }
    }
}
impl NoticeApi for FakeApi {
    fn live(&self, s: &Subject<'_>) -> Result<Member, String> {
        if self.fail_read.get() {
            return Err("API unavailable".into());
        }
        s.member(&self.state.borrow())
    }
    fn seen(&self, s: &Subject<'_>, viewer: &str) -> Result<bool, String> {
        already_posted(&self.comments.borrow(), &s.marker(), viewer)
    }
    fn post(&self, _: &Subject<'_>, body: &str) -> Result<u64, String> {
        self.writes.set(self.writes.get() + 1);
        *self.comments.borrow_mut() =
            json!([[{"body":body,"user":{"login":"maintainer"},"author_association":"MEMBER"}]]);
        if self.lose_acknowledgement {
            return Err("server accepted the comment but the reply was lost".into());
        }
        Ok(47)
    }
}
#[test]
fn explicit_notice_rechecks_membership_and_suppresses_the_next_serial_post() -> Result<(), String> {
    let s = subject();
    let initial = s.member(&membership())?;
    let api = FakeApi::new(false);
    let body = advisory(&s, &conflict()).ok_or("missing fixture notice")?;
    if deliver(&api, &s, &initial, Some(&body), false)? != Disposition::Preview
        || deliver(&api, &s, &initial, None, true)? != Disposition::Unproved
    {
        return Err("unexpected preview/unexamined decision".into());
    }
    api.fail_read.set(true);
    if deliver(&api, &s, &initial, Some(&body), true).is_ok() {
        return Err("attempted write after failed final read".into());
    }
    api.fail_read.set(false);
    api.state.borrow_mut()["data"]["repository"]["pullRequest"]["mergeQueueEntry"]["id"] =
        json!("reentered");
    if deliver(&api, &s, &initial, Some(&body), true).is_ok() {
        return Err("old evidence survived dequeue/requeue".into());
    }
    *api.state.borrow_mut() = membership();
    *api.comments.borrow_mut() =
        json!([[{"body":body,"user":{"login":"untrusted"},"author_association":"NONE"}]]);
    if api.writes.get() != 0
        || deliver(&api, &s, &initial, Some(&body), true)? != Disposition::Posted(47)
        || deliver(&api, &s, &initial, Some(&body), true)? != Disposition::AlreadyPosted
        || api.writes.get() != 1
    {
        return Err("missing or duplicated serial comment".into());
    }
    if already_posted(
        &json!({"message":"rate limited"}),
        &s.marker(),
        "maintainer",
    )
    .is_ok()
    {
        return Err("treated unreadable history as empty".into());
    }
    Ok(())
}

#[test]
fn accepted_comment_with_missing_acknowledgement_is_found_on_retry() -> Result<(), String> {
    let target = subject();
    let initial = target.member(&membership())?;
    let body = advisory(&target, &conflict()).ok_or("missing fixture notice")?;
    let api = FakeApi::new(true);
    if deliver(&api, &target, &initial, Some(&body), true).is_ok()
        || deliver(&api, &target, &initial, Some(&body), true)? != Disposition::AlreadyPosted
        || api.writes.get() != 1
    {
        return Err("an unacknowledged remote write must be discovered before retry".into());
    }
    Ok(())
}

#[test]
fn failed_intent_commit_prevents_submission() -> Result<(), Box<dyn std::error::Error>> {
    let dir = crate::tests::temp_dir("notice-intent-fault");
    let target = subject();
    let api = FakeApi::new(false);
    fs::create_dir(dir.join("queue-conflict-notice-pending.json"))?;
    let initial = target.member(&membership())?;
    let result = record_delivery(&dir, &api, &target, &initial, &conflict(), true);
    let error = result.err().ok_or("accepted an uncommitted intent")?;
    if !error.contains("comment not submitted") || api.writes.get() != 0 {
        return Err(error.into());
    }
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn refused_final_reads_are_recorded_as_not_submitted() -> Result<(), Box<dyn std::error::Error>> {
    let target = subject();
    let initial = target.member(&membership())?;
    for failure in [
        "membership unavailable",
        "entry replaced",
        "comment history unavailable",
    ] {
        let dir = crate::tests::temp_dir("notice-read-fault");
        let api = FakeApi::new(false);
        match failure {
            "membership unavailable" => api.fail_read.set(true),
            "entry replaced" => {
                api.state.borrow_mut()["data"]["repository"]["pullRequest"]["mergeQueueEntry"]["id"] =
                    json!("replacement")
            }
            _ => *api.comments.borrow_mut() = Value::Null,
        }
        let result = record_delivery(&dir, &api, &target, &initial, &conflict(), true);
        let receipt: Value =
            serde_json::from_slice(&fs::read(dir.join("queue-conflict-notice.json"))?)?;
        if result.is_ok() || api.writes.get() != 0 || receipt["disposition"] != "not_submitted" {
            return Err(format!("wrong receipt for {failure}: {receipt}").into());
        }
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn failed_terminal_commit_preserves_intent_and_remote_outcome()
-> Result<(), Box<dyn std::error::Error>> {
    let target = subject();
    let initial = target.member(&membership())?;
    let row = conflict();
    let body = advisory(&target, &row).ok_or("missing fixture notice")?;
    let expected = serde_json::to_vec_pretty(
        &json!({"schema_version":1,"repository":REPO,"main":MAIN,"entry":"first","row":row,"body":body,"post_requested":true,"disposition":"pending"}),
    )?;
    for lost_ack in [false, true] {
        let api = FakeApi::new(lost_ack);
        let dir = crate::tests::temp_dir("notice-terminal-fault");
        fs::create_dir(dir.join("queue-conflict-notice.json"))?;
        let result = record_delivery(&dir, &api, &target, &initial, &row, true);
        let error = result
            .err()
            .ok_or("terminal receipt unexpectedly committed")?;
        let remote = if lost_ack {
            "server accepted the comment but the reply was lost"
        } else {
            r#"{"posted":47}"#
        };
        if !error.contains(remote)
            || !error.contains("could not commit")
            || api.writes.get() != 1
            || fs::read(dir.join("queue-conflict-notice-pending.json"))? != expected
        {
            return Err(format!("incomplete outcome after terminal failure: {error}").into());
        }
        let retry = crate::tests::temp_dir("notice-after-terminal-fault");
        record_delivery(&retry, &api, &target, &initial, &row, false)?;
        let receipt: Value =
            serde_json::from_slice(&fs::read(retry.join("queue-conflict-notice.json"))?)?;
        if receipt["disposition"] != "already_posted" || api.writes.get() != 1 {
            return Err(
                "read-only reconciliation did not find the previously posted notice".into(),
            );
        }
        fs::remove_dir_all(retry)?;
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}
