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
    let api = FakeApi {
        state: RefCell::new(membership()),
        comments: RefCell::new(json!([[]])),
        writes: Cell::new(0),
        fail_read: Cell::new(false),
        lose_acknowledgement: false,
    };
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
    let api = FakeApi {
        state: RefCell::new(membership()),
        comments: RefCell::new(json!([[]])),
        writes: Cell::new(0),
        fail_read: Cell::new(false),
        lose_acknowledgement: true,
    };
    if deliver(&api, &target, &initial, Some(&body), true).is_ok()
        || deliver(&api, &target, &initial, Some(&body), true)? != Disposition::AlreadyPosted
        || api.writes.get() != 1
    {
        return Err("an unacknowledged remote write must be discovered before retry".into());
    }
    Ok(())
}
