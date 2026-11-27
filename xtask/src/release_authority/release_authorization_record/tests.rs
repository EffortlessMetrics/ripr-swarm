//! Durable-record controls, driven through the production verifier with an
//! in-memory reader. The numbered comments are the ripr#1651 negative controls.

use std::cell::Cell;

use serde_json::{Value as Json, json};

use super::super::authorizer_policy::AuthorizerPolicy;
use super::super::github_reader::RepositoryObservation;
use super::super::release_authorization_packet::render_comment;
use super::super::release_authorization_packet::tests::{
    ISSUER_ID, ISSUER_LOGIN, KINDS, Kind, NONCE, RECORD_ISSUE, REPOSITORY, REPOSITORY_ID, draft,
    seal, sha,
};
use super::*;

const POLICY: &str = include_str!("../../../../policy/release-authorizers.toml");
const COMMENT_ID: u64 = 9001;
const CREATED: &str = "2026-11-21T06:00:00Z";

fn now() -> u64 {
    parse_utc_seconds("2026-11-21T08:00:00Z").unwrap_or_default()
}

/// What a failing read returns, per reader operation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fail {
    Repository,
    Comment,
    Index,
}

struct World {
    repository: RepositoryObservation,
    comments: Vec<CommentObservation>,
    fail: Option<(Fail, &'static str)>,
    /// What the issue listing returns instead of the comments on the issue.
    index: Option<Vec<CommentObservation>>,
    source: &'static str,
    reads: Cell<usize>,
}

impl RecordReader for World {
    fn source(&self) -> &'static str {
        self.source
    }

    fn repository(&self, _name: &str) -> Result<RepositoryObservation, Unavailable> {
        self.reads.set(self.reads.get() + 1);
        self.failing(Fail::Repository)?;
        Ok(self.repository.clone())
    }

    fn comment(&self, _repository: &str, id: u64) -> Result<CommentObservation, Unavailable> {
        self.reads.set(self.reads.get() + 1);
        self.failing(Fail::Comment)?;
        self.comments
            .iter()
            .find(|comment| comment.id == id)
            .cloned()
            .ok_or_else(|| Unavailable::new("http_404_or_inaccessible", "GET comment failed"))
    }

    fn issue_comments(
        &self,
        _repository: &str,
        issue: u64,
    ) -> Result<Vec<CommentObservation>, Unavailable> {
        self.reads.set(self.reads.get() + 1);
        self.failing(Fail::Index)?;
        if let Some(index) = &self.index {
            return Ok(index.clone());
        }
        Ok(self
            .comments
            .iter()
            .filter(|comment| comment.issue == issue)
            .cloned()
            .collect())
    }
}

impl World {
    fn failing(&self, operation: Fail) -> Result<(), Unavailable> {
        match self.fail {
            Some((failing, kind)) if failing == operation => {
                Err(Unavailable::new(kind, "read failed"))
            }
            _ => Ok(()),
        }
    }
}

fn comment(id: u64, issue: u64, body: String) -> CommentObservation {
    CommentObservation {
        id,
        issue,
        author_id: ISSUER_ID,
        author_login: ISSUER_LOGIN.to_string(),
        created_at: CREATED.to_string(),
        updated_at: CREATED.to_string(),
        body,
    }
}

fn record_body(draft: &Json) -> Result<String, String> {
    let canonical = seal(draft)?;
    Ok(render_comment(&Packet::parse(&canonical)?, &canonical))
}

fn bindings_for(draft: &Json) -> Result<Bindings, String> {
    let text = |pointer: &str| draft.pointer(pointer).cloned().unwrap_or(Json::Null);
    let mut evidence = draft["evidence"].clone();
    if let Some(map) = evidence.as_object_mut() {
        map.remove("policy_ruling");
    }
    Bindings::parse(
        json!({
            "profile": text("/profile"),
            "version": text("/version"),
            "source_commit": text("/source/commit"),
            "source_tree": text("/source/tree"),
            "workflow_path": text("/workflow/path"),
            "workflow_blob": text("/workflow/blob"),
            "workflow_commit": text("/workflow/commit"),
            "plan_schema": text("/workflow/plan_schema"),
            "plan_digest": text("/workflow/plan_digest"),
            "transaction_kind": text("/transaction/kind"),
            "transaction_schema": text("/transaction/schema"),
            "transaction_tool": text("/transaction/tool"),
            "transaction_request_id": text("/transaction/request_id"),
            "event_ref": text("/event/ref"),
            "evidence": evidence,
            "policy_ruling": text("/evidence/policy_ruling"),
            "subjects": text("/subjects"),
        })
        .to_string()
        .as_bytes(),
    )
}

struct Case {
    world: World,
    policy: AuthorizerPolicy,
    locator: Locator,
    bindings: Bindings,
    authorization_id: String,
}

const RULING_BODY: &str = "Owner ruling: v0.11.0-rc.1 may ship as a GitHub prerelease only.";

fn case(kind: Kind) -> Result<Case, String> {
    let mut draft = draft(kind);
    let mut comments = Vec::new();
    if kind == Kind::PublicRc {
        draft["evidence"]["policy_ruling"]["body_sha256"] = json!(digest(RULING_BODY.as_bytes()));
        comments.push(comment(4242, 1634, RULING_BODY.to_string()));
    }
    let body = record_body(&draft)?;
    let locator = Locator {
        repository_id: REPOSITORY_ID,
        repository_name: REPOSITORY.to_string(),
        issue: RECORD_ISSUE,
        comment_id: COMMENT_ID,
        body_sha256: sha256_hex(body.as_bytes()),
    };
    let authorization_id = Packet::parse(&seal(&draft)?)?.authorization_id;
    comments.push(comment(COMMENT_ID, RECORD_ISSUE, body));
    comments.push(comment(
        9002,
        RECORD_ISSUE,
        "Unrelated discussion.".to_string(),
    ));
    Ok(Case {
        world: World {
            repository: RepositoryObservation {
                id: REPOSITORY_ID,
                full_name: REPOSITORY.to_string(),
            },
            comments,
            fail: None,
            index: None,
            source: "github_api",
            reads: Cell::new(0),
        },
        policy: AuthorizerPolicy::parse(POLICY)?,
        locator,
        bindings: bindings_for(&draft)?,
        authorization_id,
    })
}

impl Case {
    fn verify(&self) -> Receipt {
        verify(
            &self.world,
            &self.policy,
            &self.locator,
            &self.bindings,
            &now,
        )
    }

    fn record(&mut self) -> Result<&mut CommentObservation, String> {
        self.world
            .comments
            .iter_mut()
            .find(|comment| comment.id == COMMENT_ID)
            .ok_or_else(|| "the case lost its record".to_string())
    }

    /// Replaces the record body and re-pins the locator to the new bytes, so a
    /// later rejection is about the content and not about the body digest.
    fn repost(&mut self, body: String) -> Result<(), String> {
        self.locator.body_sha256 = sha256_hex(body.as_bytes());
        self.record()?.body = body;
        Ok(())
    }

    fn verdict(
        &self,
        label: &str,
        disposition: Disposition,
        check: &str,
    ) -> Result<Receipt, String> {
        let receipt = self.verify();
        let state = check_state(&receipt, check);
        let expected_state = match disposition {
            Disposition::Authorized => "passed",
            Disposition::Unavailable => "unavailable",
            _ => "rejected",
        };
        if receipt.disposition != disposition || state != expected_state {
            return Err(format!(
                "{label}: disposition {} with {check}={state}; expected {} with {check}={expected_state}. reasons: {:?}",
                receipt.disposition.as_str(),
                disposition.as_str(),
                receipt.reasons
            ));
        }
        if (disposition == Disposition::Authorized) != receipt.admits() {
            return Err(format!("{label}: admits() disagrees with the disposition"));
        }
        Ok(receipt)
    }
}

fn check_state(receipt: &Receipt, id: &str) -> String {
    receipt.value["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|check| check["id"] == id)
        .and_then(|check| check["state"].as_str())
        .unwrap_or("absent")
        .to_string()
}

#[test]
fn release_authorization_record_exact_record_is_authorized_for_every_boundary() -> Result<(), String>
{
    for kind in KINDS {
        let case = case(kind)?;
        let receipt = case.verdict(
            &format!("{kind:?}"),
            Disposition::Authorized,
            "supersession",
        )?;
        if receipt.authorization_id.as_deref() != Some(case.authorization_id.as_str()) {
            return Err(format!("{kind:?}: receipt names another authorization"));
        }
        let unevaluated = receipt.value["checks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|check| check["state"] == "not_evaluated")
            .count();
        if unevaluated != 0
            || receipt.value["checks"].as_array().map(Vec::len) != Some(CHECKS.len())
        {
            return Err(format!("{kind:?}: an authorized receipt skipped a check"));
        }
        let ruling = check_state(&receipt, "policy_ruling");
        let expected = if kind == Kind::PublicRc {
            "passed"
        } else {
            "not_applicable"
        };
        if ruling != expected {
            return Err(format!("{kind:?}: policy_ruling is {ruling}"));
        }
        if receipt.value["external_mutation_attempts"] != false
            || receipt.value["credential_requests"] != false
        {
            return Err("a receipt reported a mutation or credential request".to_string());
        }
    }
    Ok(())
}

// 1, 2: prose, an unmarked block, two marked blocks, noncanonical bytes.
#[test]
fn release_authorization_record_only_one_marked_canonical_block_is_a_packet() -> Result<(), String>
{
    let base = case(Kind::StableCrates)?;
    let body = base
        .world
        .comments
        .iter()
        .find(|c| c.id == COMMENT_ID)
        .map(|c| c.body.clone())
        .ok_or("missing record")?;
    let line = body
        .lines()
        .nth(2)
        .ok_or("missing packet line")?
        .to_string();
    let pretty = serde_json::from_str::<Json>(&line)
        .and_then(|value| serde_json::to_string(&value))
        .map_err(|e| e.to_string())?
        .replacen(':', ": ", 1);
    let variants: [(&str, String, &str); 7] = [
        (
            "prose",
            "I authorize crates.io publication of 0.11.0 at the ship commit.".to_string(),
            "packet_block",
        ),
        (
            "unmarked block",
            body.replacen(RECORD_MARKER, "", 1),
            "packet_block",
        ),
        (
            "two marked blocks",
            format!("{body}\n{body}"),
            "packet_block",
        ),
        (
            "marker with trailing text",
            body.replacen(RECORD_MARKER, &format!("{RECORD_MARKER} ok"), 1),
            "packet_block",
        ),
        (
            "second unmarked candidate",
            format!("{body}\n```json\n{line}\n```\n"),
            "packet_block",
        ),
        (
            "future marker version",
            body.replacen(":v1 -->", ":v2 -->", 1),
            "packet_block",
        ),
        (
            "noncanonical bytes",
            body.replacen(&line, &pretty, 1),
            "packet_canonical_bytes",
        ),
    ];
    for (label, variant, check) in variants {
        if variant == body {
            return Err(format!("{label}: the control edit changed nothing"));
        }
        let mut case = case(Kind::StableCrates)?;
        case.repost(variant)?;
        case.verdict(label, Disposition::Malformed, check)?;
    }
    // CRLF line endings, as a browser may submit them, are still one block.
    let mut crlf = case(Kind::StableCrates)?;
    crlf.repost(body.replace('\n', "\r\n"))?;
    crlf.verdict("crlf", Disposition::Authorized, "supersession")?;
    Ok(())
}

// 3, 4: wrong repository, issue, comment or body hash; edited or missing record.
#[test]
fn release_authorization_record_locator_must_match_the_fetched_record() -> Result<(), String> {
    let mut other_repository = case(Kind::StableCrates)?;
    other_repository.world.repository.id += 1;
    other_repository.verdict(
        "repository id",
        Disposition::Mismatched,
        "repository_identity",
    )?;
    let mut renamed = case(Kind::StableCrates)?;
    renamed.world.repository.full_name = "EffortlessMetrics/ripr-swarm".to_string();
    renamed.verdict(
        "repository name",
        Disposition::Mismatched,
        "repository_identity",
    )?;

    let mut other_issue = case(Kind::StableCrates)?;
    other_issue.locator.issue = 1463;
    other_issue.verdict("issue", Disposition::Mismatched, "record_location")?;
    let mut moved = case(Kind::StableCrates)?;
    moved.record()?.issue = 1463;
    moved.verdict(
        "comment on another issue",
        Disposition::Mismatched,
        "record_location",
    )?;

    let mut other_hash = case(Kind::StableCrates)?;
    other_hash.locator.body_sha256 = "0".repeat(64);
    other_hash.verdict("body hash", Disposition::Mismatched, "record_body_digest")?;
    let mut appended = case(Kind::StableCrates)?;
    appended.record()?.body.push_str("\nAlso npm.");
    appended.verdict("edited body", Disposition::Mismatched, "record_body_digest")?;
    let mut touched = case(Kind::StableCrates)?;
    touched.record()?.updated_at = "2026-11-21T07:00:00Z".to_string();
    touched.verdict(
        "edited with identical bytes",
        Disposition::Mismatched,
        "record_unedited",
    )?;

    let mut missing = case(Kind::StableCrates)?;
    missing.locator.comment_id = 123;
    missing.verdict(
        "missing or deleted record",
        Disposition::Unavailable,
        "record_location",
    )?;
    let mut bad_locator = case(Kind::StableCrates)?;
    bad_locator.locator.repository_name = "EffortlessMetrics/ripr/issues".to_string();
    let receipt = bad_locator.verdict("malformed locator", Disposition::Mismatched, "locator")?;
    if bad_locator.world.reads.get() != 0
        || check_state(&receipt, "repository_identity") != "not_evaluated"
    {
        return Err("a malformed locator still reached the reader".to_string());
    }
    Ok(())
}

// 5, 6, 8: only the policy's stable user id authorizes; a packet cannot add one.
#[test]
fn release_authorization_record_only_the_policy_user_id_authorizes() -> Result<(), String> {
    // The same login and packet from another account id.
    let mut impostor = case(Kind::StableCrates)?;
    impostor.record()?.author_id = 42;
    impostor.verdict("login-only match", Disposition::Denied, "issuer_identity")?;

    // A packet that names its own, unlisted issuer consistently.
    let mut self_issued_draft = draft(Kind::StableCrates);
    self_issued_draft["issuer"] = json!({"user_id": 42, "login": "release-bot"});
    let mut self_issued = case(Kind::StableCrates)?;
    self_issued.repost(record_body(&self_issued_draft)?)?;
    {
        let record = self_issued.record()?;
        record.author_id = 42;
        record.author_login = "release-bot".to_string();
    }
    self_issued.verdict(
        "self-added issuer",
        Disposition::Denied,
        "authorizer_policy",
    )?;

    // The listed user id under a login the policy does not expect.
    let mut renamed_draft = draft(Kind::StableCrates);
    renamed_draft["issuer"]["login"] = json!("SomeoneElse");
    let mut renamed = case(Kind::StableCrates)?;
    renamed.repost(record_body(&renamed_draft)?)?;
    renamed.record()?.author_login = "SomeoneElse".to_string();
    renamed.verdict("login drift", Disposition::Denied, "authorizer_policy")?;

    // Removing the authorizer, or narrowing its profiles, removes the authority.
    let mut removed = case(Kind::StableCrates)?;
    removed.policy = AuthorizerPolicy::parse(&POLICY.replace("15812269", "15812270"))?;
    removed.verdict(
        "removed authorizer",
        Disposition::Denied,
        "authorizer_policy",
    )?;
    let mut narrowed = case(Kind::StableCrates)?;
    narrowed.policy = AuthorizerPolicy::parse(&POLICY.replace("\"stable_core\", ", ""))?;
    narrowed.verdict(
        "profile not granted",
        Disposition::Denied,
        "authorizer_policy",
    )?;
    let mut overdue = case(Kind::StableCrates)?;
    overdue.policy.review_deadline = now();
    overdue.verdict(
        "policy past review",
        Disposition::Denied,
        "authorizer_policy",
    )?;
    let mut elsewhere = case(Kind::StableCrates)?;
    elsewhere.policy.repository_id += 1;
    elsewhere.verdict(
        "policy for another repository",
        Disposition::Mismatched,
        "repository_identity",
    )?;
    Ok(())
}

// 7: every exact identity is bound to an independently supplied input.
#[test]
fn release_authorization_record_every_identity_is_bound() -> Result<(), String> {
    let pointers = [
        ("/version", json!("0.11.1"), "version"),
        ("/source/commit", json!("f".repeat(40)), "source_commit"),
        ("/source/tree", json!("f".repeat(40)), "source_tree"),
        (
            "/evidence/ship_packet",
            json!(sha('f')),
            "evidence.ship_packet",
        ),
        (
            "/evidence/stable_decision",
            json!(sha('f')),
            "evidence.stable_decision",
        ),
        (
            "/evidence/qualification",
            json!(sha('f')),
            "evidence.qualification",
        ),
        ("/workflow/blob", json!("f".repeat(40)), "workflow_blob"),
        ("/workflow/commit", json!("f".repeat(40)), "workflow_commit"),
        ("/workflow/plan_digest", json!(sha('f')), "plan_digest"),
        (
            "/workflow/plan_schema",
            json!("ripr-release-plan/v2"),
            "plan_schema",
        ),
        (
            "/transaction/request_id",
            json!("crates_io-request-2"),
            "transaction_request_id",
        ),
        (
            "/transaction/schema",
            json!("ripr-crates_io-transaction/v2"),
            "transaction_schema",
        ),
        ("/subjects/0/sha256", json!(sha('f')), "subjects"),
        (
            "/subjects/0/name",
            json!("ripr-contract-0.11.0.crate"),
            "subjects",
        ),
    ];
    for (pointer, value, field) in pointers {
        // The packet was authorized for one input; the caller holds another.
        let mut case = case(Kind::StableCrates)?;
        let mut other = draft(Kind::StableCrates);
        *other.pointer_mut(pointer).ok_or("bad control pointer")? = value;
        if pointer == "/version" {
            // Not a packet change: only the caller's expectation moves.
            other["event"]["ref"] = Json::Null;
        }
        case.bindings = bindings_for(&other)?;
        let receipt = case.verdict(pointer, Disposition::Mismatched, "bindings")?;
        if !receipt.reasons.iter().any(|reason| reason.contains(field)) {
            return Err(format!(
                "{pointer}: mismatch does not name {field}: {:?}",
                receipt.reasons
            ));
        }
    }
    let mut tag = case(Kind::StableGithub)?;
    let mut other = draft(Kind::StableGithub);
    other["event"]["ref"] = json!("refs/tags/v0.11.1");
    tag.bindings = bindings_for(&other)?;
    tag.verdict("event ref", Disposition::Mismatched, "bindings")?;

    // The cited ruling is an input too: a packet that cites a real comment by
    // an authorizer, but not the ruling the caller holds, is not this release.
    for edit in [
        (|d: &mut Json| d["evidence"]["policy_ruling"]["comment_id"] = json!(4243))
            as fn(&mut Json),
        |d| d["evidence"]["policy_ruling"]["body_sha256"] = json!(sha('f')),
        |d| d["evidence"]["policy_ruling"] = Json::Null,
    ] {
        let mut rc = case(Kind::PublicRc)?;
        rc.verdict("rc baseline", Disposition::Authorized, "policy_ruling")?;
        let mut other = draft(Kind::PublicRc);
        edit(&mut other);
        rc.bindings = bindings_for(&other)?;
        let receipt = rc.verdict("cited ruling", Disposition::Mismatched, "bindings")?;
        if !receipt.reasons.iter().any(|r| r.contains("policy_ruling")) {
            return Err(format!(
                "ruling mismatch is not named: {:?}",
                receipt.reasons
            ));
        }
    }
    Ok(())
}

// 9-13 at the record level: a profile violation never reaches authorization.
#[test]
fn release_authorization_record_profile_violations_are_malformed_records() -> Result<(), String> {
    let mut case = case(Kind::PublicRc)?;
    let body = case.record()?.body.clone();
    // Hand-edit the posted packet to add crates.io authority.
    let widened = body.replacen(
        "\"crates_io\":\"not_authorized\"",
        "\"crates_io\":\"authorized\"",
        1,
    );
    if widened == body {
        return Err("the control edit changed nothing".to_string());
    }
    case.repost(widened)?;
    case.verdict(
        "rc widened to crates.io",
        Disposition::Malformed,
        "packet_fields_and_profile",
    )?;

    // A consistent edit to an in-profile field still breaks the identity.
    let mut retargeted = case_with_body_edit(
        Kind::StableCrates,
        "crates_io-request-1",
        "crates_io-request-9",
    )?;
    retargeted.bindings = retargeted.bindings.clone();
    retargeted.verdict(
        "retargeted request",
        Disposition::Malformed,
        "authorization_id",
    )?;
    Ok(())
}

fn case_with_body_edit(kind: Kind, from: &str, to: &str) -> Result<Case, String> {
    let mut case = case(kind)?;
    let body = case.record()?.body.clone();
    let edited = body.replacen(from, to, 1);
    if edited == body {
        return Err(format!("control edit `{from}` changed nothing"));
    }
    case.repost(edited)?;
    Ok(case)
}

// RC: the cited ruling is re-read as exact bytes from an authorizer.
#[test]
fn release_authorization_record_rc_ruling_is_reread_not_parsed() -> Result<(), String> {
    let ruling = |case: &mut Case| -> Result<usize, String> {
        case.world
            .comments
            .iter()
            .position(|comment| comment.id == 4242)
            .ok_or_else(|| "missing ruling".to_string())
    };
    let mut edited = case(Kind::PublicRc)?;
    let at = ruling(&mut edited)?;
    edited.world.comments[at].body.push_str(" And crates.io.");
    edited.verdict("edited ruling", Disposition::Mismatched, "policy_ruling")?;
    let mut foreign = case(Kind::PublicRc)?;
    let at = ruling(&mut foreign)?;
    foreign.world.comments[at].author_id = 42;
    foreign.verdict(
        "ruling by a non-authorizer",
        Disposition::Denied,
        "policy_ruling",
    )?;
    let mut relocated = case(Kind::PublicRc)?;
    let at = ruling(&mut relocated)?;
    relocated.world.comments[at].issue = 1;
    relocated.verdict(
        "ruling on another issue",
        Disposition::Mismatched,
        "policy_ruling",
    )?;
    let mut gone = case(Kind::PublicRc)?;
    let at = ruling(&mut gone)?;
    gone.world.comments.remove(at);
    gone.verdict(
        "ruling unavailable",
        Disposition::Unavailable,
        "policy_ruling",
    )?;
    Ok(())
}

// 14: outside the validity window.
#[test]
fn release_authorization_record_validity_window_is_enforced() -> Result<(), String> {
    let case = case(Kind::StableCrates)?;
    let at = |text: &str| parse_utc_seconds(text).ok_or_else(|| "bad time".to_string());
    let run = |now: u64| {
        verify(
            &case.world,
            &case.policy,
            &case.locator,
            &case.bindings,
            &|| now,
        )
    };
    let before = run(at("2026-11-20T23:59:59Z")?);
    if before.disposition != Disposition::Denied
        || check_state(&before, "validity_window") != "rejected"
    {
        return Err("a not-yet-valid packet was not denied".to_string());
    }
    let first = run(at("2026-11-21T00:00:00Z")?);
    let last = run(at("2026-11-27T23:59:59Z")?);
    if first.disposition != Disposition::Authorized || last.disposition != Disposition::Authorized {
        return Err("the window boundaries are not inclusive-start".to_string());
    }
    let after = run(at("2026-11-28T00:00:00Z")?);
    if after.disposition != Disposition::Expired
        || check_state(&after, "supersession") != "not_evaluated"
    {
        return Err("an expired packet was not expired".to_string());
    }
    Ok(())
}

fn superseder(sequence: u64, edit: fn(&mut Json), target: &str) -> Result<String, String> {
    let mut later = draft(Kind::StableCrates);
    later["sequence"] = json!(sequence);
    later["nonce"] = json!("fedcba9876543210fedcba9876543210");
    later["supersedes"] = json!(target);
    edit(&mut later);
    record_body(&later)
}

fn revocation(target: &str, edit: fn(&mut Json)) -> Result<String, String> {
    let mut draft = json!({
        "schema": REVOCATION_SCHEMA,
        "repository": {"id": REPOSITORY_ID, "name": REPOSITORY},
        "programme_issue": 1463,
        "record": {"issue": RECORD_ISSUE, "purpose": REVOCATION_PURPOSE},
        "issuer": {"user_id": ISSUER_ID, "login": ISSUER_LOGIN},
        "revokes": target,
        "nonce": NONCE,
        "created_at": "2026-11-21T07:00:00Z",
    });
    edit(&mut draft);
    render_revocation(draft.to_string().as_bytes()).map(|(_, comment)| comment)
}

// 15: a malformed or unauthorized later record cannot disable a valid packet.
#[test]
fn release_authorization_record_invalid_supersession_changes_nothing() -> Result<(), String> {
    let id = case(Kind::StableCrates)?.authorization_id;
    let valid = superseder(2, |_| {}, &id)?;
    let valid_revocation = revocation(&id, |_| {})?;
    type Mutate = fn(&mut CommentObservation);
    // The flag says whether the receipt must list the attempt as ignored: a
    // record that names this packet but is not a valid authorized record.
    let attempts: Vec<(&str, String, Mutate, bool)> = vec![
        (
            "prose cancellation",
            format!("Cancelled. Do not use {id}."),
            |_| {},
            false,
        ),
        (
            "record of another family naming this packet",
            format!(
                "<!-- ripr-release-authorization-consumption:v1 -->\n```json\n{{\"authorization_id\":\"{id}\"}}\n```\n"
            ),
            |_| {},
            false,
        ),
        (
            "packet record of an unknown version naming this packet",
            valid.replacen("authorization:v1 -->", "authorization:v2 -->", 1),
            |_| {},
            true,
        ),
        (
            "unauthorized author",
            valid.clone(),
            |c| c.author_id = 42,
            true,
        ),
        (
            "edited supersessor",
            valid.clone(),
            |c| c.updated_at = "2026-11-21T07:30:00Z".to_string(),
            true,
        ),
        (
            "noncanonical supersessor",
            valid.replacen("\"sequence\":2", "\"sequence\": 2", 1),
            |_| {},
            true,
        ),
        (
            "not a later sequence",
            superseder(1, |_| {}, &id)?,
            |_| {},
            true,
        ),
        (
            "another transaction boundary",
            {
                let mut later = draft(Kind::StableGithub);
                later["sequence"] = json!(2);
                later["supersedes"] = json!(id.clone());
                record_body(&later)?
            },
            |_| {},
            true,
        ),
        (
            "supersessor on another issue",
            valid.clone(),
            |c| c.issue = 1463,
            false,
        ),
        (
            "unauthorized revocation",
            valid_revocation.clone(),
            |c| c.author_id = 42,
            true,
        ),
        (
            "edited revocation",
            valid_revocation.clone(),
            |c| c.updated_at = "2026-11-21T07:30:00Z".to_string(),
            true,
        ),
        (
            "revocation of another programme",
            revocation(&id, |d| d["programme_issue"] = json!(1))?,
            |_| {},
            true,
        ),
        (
            "supersessor with a pasted credential",
            format!("{valid}\nToken: ghp_{}\n", "Zx9Qw3Er".repeat(5)),
            |_| {},
            true,
        ),
        (
            "supersessor naming another issuer login",
            superseder(2, |d| d["issuer"]["login"] = json!("SomeoneElse"), &id)?,
            |_| {},
            true,
        ),
        (
            "revocation with a pasted credential",
            format!("{valid_revocation}\nToken: ghp_{}\n", "Zx9Qw3Er".repeat(5)),
            |_| {},
            true,
        ),
        (
            "revocation naming another issuer login",
            revocation(&id, |d| d["issuer"]["login"] = json!("SomeoneElse"))?,
            |_| {},
            true,
        ),
        (
            "revocation naming another packet",
            revocation(&sha('0'), |_| {})?,
            |_| {},
            false,
        ),
    ];
    for (label, body, mutate, listed) in attempts {
        let mut case = case(Kind::StableCrates)?;
        let mut later = comment(9100, RECORD_ISSUE, body);
        mutate(&mut later);
        case.world.comments.push(later);
        let receipt = case.verdict(label, Disposition::Authorized, "supersession")?;
        let ignored = receipt.value["supersession"]["ignored_candidates"]
            .as_array()
            .is_some_and(|list| list.iter().any(|entry| entry["comment_id"] == 9100));
        if ignored != listed {
            return Err(format!("{label}: ignored-candidate listing is {ignored}"));
        }
    }
    Ok(())
}

// 16: a valid supersession or revocation disables exactly the named packet.
#[test]
fn release_authorization_record_valid_supersession_disables_only_its_target() -> Result<(), String>
{
    let id = case(Kind::StableCrates)?.authorization_id;
    let mut superseded = case(Kind::StableCrates)?;
    superseded
        .world
        .comments
        .push(comment(9100, RECORD_ISSUE, superseder(2, |_| {}, &id)?));
    let receipt = superseded.verdict("superseded", Disposition::Superseded, "supersession")?;
    if receipt.value["supersession"]["superseded_by"]
        .as_str()
        .is_none_or(|by| by == id)
    {
        return Err("the receipt does not name the superseding authorization".to_string());
    }

    // The same later record leaves a packet for another boundary authorized.
    let mut bystander = case(Kind::VsMarketplace)?;
    bystander
        .world
        .comments
        .push(comment(9100, RECORD_ISSUE, superseder(2, |_| {}, &id)?));
    bystander.verdict("other boundary", Disposition::Authorized, "supersession")?;

    let mut revoked = case(Kind::StableCrates)?;
    revoked
        .world
        .comments
        .push(comment(9101, RECORD_ISSUE, revocation(&id, |_| {})?));
    revoked
        .world
        .comments
        .push(comment(9100, RECORD_ISSUE, superseder(2, |_| {}, &id)?));
    let receipt = revoked.verdict("revoked", Disposition::Revoked, "supersession")?;
    if receipt.value["supersession"]["revoked_by"].is_null() {
        return Err("the receipt does not name the revocation".to_string());
    }
    Ok(())
}

// 17: no read failure is ever an authorization.
#[test]
fn release_authorization_record_unavailable_reads_are_never_authorized() -> Result<(), String> {
    let kinds = [
        "http_403",
        "http_404_or_inaccessible",
        "http_429",
        "http_5xx",
        "transport_or_tls",
        "malformed_response",
        "timeout",
        "unexpected_pagination",
        "incomplete_observation",
    ];
    let operations = [
        (Fail::Repository, "repository_identity"),
        (Fail::Comment, "record_location"),
        (Fail::Index, "supersession"),
    ];
    for kind in kinds {
        for (operation, check) in operations {
            let mut case = case(Kind::StableCrates)?;
            case.world.fail = Some((operation, kind));
            let receipt = case.verdict(kind, Disposition::Unavailable, check)?;
            if !receipt.reasons.iter().any(|reason| reason.contains(kind)) {
                return Err(format!(
                    "{kind}: the receipt does not retain the observation"
                ));
            }
        }
    }
    // An index that does not contain the record as it was fetched: missing,
    // or present with other bytes. Either way the listing is not the record's.
    for (label, edit) in [
        ("record missing from the index", None),
        (
            "record differs in the index",
            Some("edited after the fetch"),
        ),
    ] {
        let mut inconsistent = case(Kind::StableCrates)?;
        inconsistent.verdict("baseline", Disposition::Authorized, "supersession")?;
        let mut index = inconsistent.world.comments.clone();
        match edit {
            None => index.retain(|comment| comment.id != COMMENT_ID),
            Some(body) => {
                for comment in index.iter_mut().filter(|c| c.id == COMMENT_ID) {
                    comment.body = body.to_string();
                }
            }
        }
        inconsistent.world.index = Some(index);
        let receipt = inconsistent.verdict(label, Disposition::Unavailable, "supersession")?;
        if !receipt
            .reasons
            .iter()
            .any(|r| r.contains("inconsistent_index"))
        {
            return Err(format!("{label}: {:?}", receipt.reasons));
        }
    }

    // A live reader that never read anything observed no live authority.
    let mut unread = case(Kind::StableCrates)?;
    unread.locator.repository_name = "not a repository".to_string();
    let receipt = unread.verdict("invalid locator", Disposition::Mismatched, "locator")?;
    if unread.world.reads.get() != 0 || receipt.live || receipt.value["live_authority"] != false {
        return Err("a receipt with no read claims a live observation".to_string());
    }

    // A replayed fixture can be evaluated but is not live authority.
    let mut replay = case(Kind::StableCrates)?;
    replay.world.source = "saved_fixture";
    let receipt = replay.verify();
    if receipt.disposition != Disposition::Authorized || receipt.admits() {
        return Err("a replayed record was admitted as live authority".to_string());
    }
    Ok(())
}

// 18: a credential-shaped value rejects and is not echoed.
#[test]
fn release_authorization_record_secret_values_reject_and_are_withheld() -> Result<(), String> {
    let token = format!("ghp_{}", "Zx9Qw3Er".repeat(5));
    let mut pasted = case(Kind::StableCrates)?;
    let body = format!("{}\nToken for the run: {token}\n", pasted.record()?.body);
    pasted.repost(body)?;
    let receipt = pasted.verdict("token in prose", Disposition::Malformed, "secret_scan")?;
    let rendered = format!("{}{}", receipt.json(), receipt.markdown());
    if rendered.contains(&token) || !receipt.value["packet"].is_null() {
        return Err("the receipt echoed a rejected record".to_string());
    }

    let mut valued = draft(Kind::StableCrates);
    valued["environments"][0]["credentials"] = json!([token]);
    if seal(&valued)
        .and_then(|bytes| Packet::parse(&bytes))
        .is_ok()
    {
        return Err("a credential value was accepted as a credential name".to_string());
    }
    Ok(())
}

// 19: verification performs reads only.
#[test]
fn release_authorization_record_verification_only_reads() -> Result<(), String> {
    let case = case(Kind::PublicRc)?;
    let receipt = case.verdict("rc", Disposition::Authorized, "supersession")?;
    // Repository, record, cited ruling and the bounded index: four reads.
    if case.world.reads.get() != 4 {
        return Err(format!(
            "expected 4 reads, observed {}",
            case.world.reads.get()
        ));
    }
    if receipt.value["non_claims"].as_array().map(Vec::len) != Some(NON_CLAIMS.len()) {
        return Err("the receipt lost its non-claims".to_string());
    }
    Ok(())
}

// 20: the receipt identity ignores enumeration order and carries no paths.
#[test]
fn release_authorization_record_receipt_identity_is_stable() -> Result<(), String> {
    let id = case(Kind::StableCrates)?.authorization_id;
    let build = |reverse: bool| -> Result<Receipt, String> {
        let mut case = case(Kind::StableCrates)?;
        case.world
            .comments
            .push(comment(9100, RECORD_ISSUE, superseder(2, |_| {}, &id)?));
        case.world
            .comments
            .push(comment(9101, RECORD_ISSUE, superseder(3, |_| {}, &id)?));
        case.world
            .comments
            .push(comment(9102, RECORD_ISSUE, format!("Cancelled {id}")));
        if reverse {
            case.world.comments.reverse();
        }
        Ok(case.verify())
    };
    let (forward, backward) = (build(false)?, build(true)?);
    if forward.json() != backward.json() || forward.receipt_id().is_empty() {
        return Err("receipt bytes depend on comment enumeration order".to_string());
    }
    if forward.disposition != Disposition::Superseded {
        return Err("the ordering control did not exercise supersession".to_string());
    }
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    if forward.json().contains(&root.display().to_string()) || forward.json().contains("/tmp") {
        return Err("the receipt carries an absolute path".to_string());
    }
    // Lookup time is not semantic: a later read of the same record inside the
    // window has the same identity. A read that changes the verdict does not.
    let authorized = case(Kind::StableCrates)?;
    let at = |now: u64| {
        verify(
            &authorized.world,
            &authorized.policy,
            &authorized.locator,
            &authorized.bindings,
            &|| now,
        )
    };
    let (one, two) = (at(now()), at(now() + 3_600));
    if one.receipt_id() != two.receipt_id() || one.json() == two.json() {
        return Err("lookup time changed the receipt identity".to_string());
    }
    let expired = at(now() + 30 * 86_400);
    if expired.receipt_id() == one.receipt_id() || one.receipt_id() == forward.receipt_id() {
        return Err("receipt identity is not a function of the evaluated verdict".to_string());
    }
    Ok(())
}

// 14 under a moving clock: the window is decided after the last read.
#[test]
fn release_authorization_record_expiry_is_decided_after_the_last_read() -> Result<(), String> {
    let case = case(Kind::StableCrates)?;
    let at = |text: &str| parse_utc_seconds(text).ok_or_else(|| "bad time".to_string());
    let (inside, edge) = (at("2026-11-27T23:59:58Z")?, at("2026-11-28T00:00:00Z")?);
    let run = |readings: [u64; 2]| {
        let calls = Cell::new(0);
        let clock = || {
            let reading = readings[calls.get().min(1)];
            calls.set(calls.get() + 1);
            reading
        };
        let receipt = verify(
            &case.world,
            &case.policy,
            &case.locator,
            &case.bindings,
            &clock,
        );
        (receipt, calls.get())
    };
    let (steady, readings) = run([inside, inside + 1]);
    if steady.disposition != Disposition::Authorized || readings != 2 {
        return Err(format!(
            "a record still inside its window after {readings} clock readings is {}",
            steady.disposition.as_str()
        ));
    }
    if steady.value["evaluated_at_unix"] != json!(inside + 1) {
        return Err("the receipt does not carry the deciding clock reading".to_string());
    }
    let (lapsed, _) = run([inside, edge]);
    if lapsed.disposition != Disposition::Expired
        || check_state(&lapsed, "validity_window") != "rejected"
        || check_state(&lapsed, "supersession") != "not_evaluated"
        || lapsed.admits()
    {
        return Err(format!(
            "a record that expired during the reads is {}",
            lapsed.disposition.as_str()
        ));
    }
    // A clock that steps back after the start does not reopen the window.
    let (rewound, _) = run([edge, inside]);
    if rewound.disposition != Disposition::Expired {
        return Err("a rewound clock reopened an elapsed window".to_string());
    }
    // The policy review deadline is held to the same final reading.
    let mut policy = case.policy.clone();
    policy.review_deadline = inside + 1;
    let calls = Cell::new(0);
    let clock = || {
        calls.set(calls.get() + 1);
        if calls.get() == 1 { inside } else { inside + 1 }
    };
    let overdue = verify(&case.world, &policy, &case.locator, &case.bindings, &clock);
    if overdue.disposition != Disposition::Denied {
        return Err("a policy that fell due during the reads still authorized".to_string());
    }
    Ok(())
}

// 18 for revocations: a rendered comment is public, so a value never renders.
#[test]
fn release_authorization_record_revocation_draft_secrets_do_not_render() -> Result<(), String> {
    let id = case(Kind::StableCrates)?.authorization_id;
    revocation(&id, |_| {})?;
    let token = format!("ghp_{}", "Zx9Qw3Er".repeat(5));
    let mut draft = json!({
        "schema": REVOCATION_SCHEMA,
        "repository": {"id": REPOSITORY_ID, "name": REPOSITORY},
        "programme_issue": 1463,
        "record": {"issue": RECORD_ISSUE, "purpose": REVOCATION_PURPOSE},
        "issuer": {"user_id": ISSUER_ID, "login": ISSUER_LOGIN},
        "revokes": id,
        "nonce": NONCE,
        "created_at": "2026-11-21T07:00:00Z",
    });
    draft["issuer"]["login"] = json!(token.clone());
    match render_revocation(draft.to_string().as_bytes()) {
        Err(error) if error.contains("credential-shaped") && !error.contains(&token) => Ok(()),
        Err(error) => Err(format!("unexpected refusal: {error}")),
        Ok(_) => Err("a revocation carrying a token was rendered".to_string()),
    }
}
