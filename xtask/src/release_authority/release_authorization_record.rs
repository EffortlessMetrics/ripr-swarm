//! Durable-record verification of one release authorization packet
//! (ripr#1651).
//!
//! Given an exact locator, the verifier re-reads the repository, the comment
//! and the bounded comment index, and reports one disposition. It reads the
//! marked canonical block only; prose, labels, reactions, author association
//! and the comment's position are never authority. Any observation it cannot
//! complete is `unavailable`, never `authorized`.

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use super::authorizer_policy::{AuthorizerPolicy, POLICY_PATH, POLICY_SCHEMA};
use super::github_reader::{CommentObservation, RecordReader, Unavailable, valid_repository_name};
use super::release_authorization_packet::{
    Channel, EvidenceKind, INVALIDATIONS, NON_CLAIMS, Obj, Operation, PACKET_SCHEMA, Packet,
    Profile, RECORD_MARKER, Subject, SubjectClass, TransactionKind, identity_of, is_digest,
    is_git_object,
};
use super::{digest, parse_canonical, parse_utc_seconds, secret_shaped, sha256_hex};

pub(crate) const RECEIPT_SCHEMA: &str = "ripr-release-authorization-receipt/v1";
pub(crate) const REVOCATION_SCHEMA: &str = "ripr-release-authorization-revocation/v1";
pub(crate) const REVOCATION_MARKER: &str = "<!-- ripr-release-authorization-revocation:v1 -->";
const REVOCATION_PURPOSE: &str = "owner_release_authorization_revocation";
/// Any marker of this family, whatever its suffix or version.
const MARKER_FAMILY: &str = "<!-- ripr-release-authorization";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Disposition {
    Authorized,
    Denied,
    Expired,
    Superseded,
    Revoked,
    Unavailable,
    Malformed,
    Mismatched,
}

impl Disposition {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Superseded => "superseded",
            Self::Revoked => "revoked",
            Self::Unavailable => "unavailable",
            Self::Malformed => "malformed",
            Self::Mismatched => "mismatched",
        }
    }
}

/// Every check, in evaluation order. A check after the first one that does not
/// pass is reported `not_evaluated`.
const CHECKS: &[&str] = &[
    "locator",
    "repository_identity",
    "record_location",
    "record_body_digest",
    "record_unedited",
    "secret_scan",
    "packet_block",
    "packet_canonical_bytes",
    "packet_fields_and_profile",
    "authorization_id",
    "record_agreement",
    "issuer_identity",
    "authorizer_policy",
    "bindings",
    "policy_ruling",
    "validity_window",
    "supersession",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Locator {
    pub(crate) repository_id: u64,
    pub(crate) repository_name: String,
    pub(crate) issue: u64,
    pub(crate) comment_id: u64,
    pub(crate) body_sha256: String,
}

/// The exact inputs the caller independently holds. Every identity the packet
/// carries is compared with one of these; the packet never vouches for itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bindings {
    profile: Profile,
    version: String,
    source_commit: String,
    source_tree: String,
    workflow_path: String,
    workflow_blob: String,
    workflow_commit: String,
    plan_schema: String,
    plan_digest: String,
    transaction_kind: TransactionKind,
    transaction_schema: String,
    transaction_tool: String,
    transaction_request_id: String,
    event_ref: Option<String>,
    evidence: BTreeMap<EvidenceKind, Option<String>>,
    subjects: Vec<Subject>,
}

impl Bindings {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let value = crate::exact_schema_json::parse(bytes)
            .map_err(|e| format!("bindings are not one JSON document: {e}"))?;
        let mut root = Obj::new(&value, "bindings")?;
        let mut text = |key: &str| root.str(key).map(str::to_string);
        let profile = text("profile")?;
        let transaction_kind = text("transaction_kind")?;
        let mut bindings = Self {
            profile: Profile::parse(&profile)
                .ok_or_else(|| format!("bindings.profile: unknown `{profile}`"))?,
            version: text("version")?,
            source_commit: text("source_commit")?,
            source_tree: text("source_tree")?,
            workflow_path: text("workflow_path")?,
            workflow_blob: text("workflow_blob")?,
            workflow_commit: text("workflow_commit")?,
            plan_schema: text("plan_schema")?,
            plan_digest: text("plan_digest")?,
            transaction_kind: TransactionKind::parse(&transaction_kind).ok_or_else(|| {
                format!("bindings.transaction_kind: unknown `{transaction_kind}`")
            })?,
            transaction_schema: text("transaction_schema")?,
            transaction_tool: text("transaction_tool")?,
            transaction_request_id: text("transaction_request_id")?,
            event_ref: None,
            evidence: BTreeMap::new(),
            subjects: Vec::new(),
        };
        bindings.event_ref = root.opt_str("event_ref")?.map(str::to_string);
        let mut evidence = root.obj("evidence")?;
        for kind in EvidenceKind::ALL {
            let value = evidence.opt_str(kind.as_str())?;
            if value.is_some_and(|text| !is_digest(text)) {
                return Err(format!(
                    "bindings.evidence.{}: expected a sha256 digest or null",
                    kind.as_str()
                ));
            }
            bindings.evidence.insert(*kind, value.map(str::to_string));
        }
        evidence.finish()?;
        for (index, item) in root.arr("subjects")?.iter().enumerate() {
            let mut subject = Obj::new(item, &format!("bindings.subjects[{index}]"))?;
            let class = subject.str("class")?;
            bindings.subjects.push(Subject {
                class: SubjectClass::parse(class).ok_or_else(|| {
                    format!("bindings.subjects[{index}].class: unknown `{class}`")
                })?,
                name: subject.str("name")?.to_string(),
                sha256: subject.str("sha256")?.to_string(),
            });
            subject.finish()?;
        }
        root.finish()?;
        if !is_git_object(&bindings.source_commit) || !is_git_object(&bindings.source_tree) {
            return Err("bindings: source_commit and source_tree are full Git object ids".into());
        }
        bindings.subjects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(bindings)
    }

    /// The names of every field where the packet and the caller disagree.
    fn mismatches(&self, packet: &Packet) -> Vec<String> {
        let mut fields = Vec::new();
        let mut compare = |name: &str, equal: bool| {
            if !equal {
                fields.push(name.to_string());
            }
        };
        compare("profile", self.profile == packet.profile);
        compare("version", self.version == packet.version);
        compare("source_commit", self.source_commit == packet.source_commit);
        compare("source_tree", self.source_tree == packet.source_tree);
        compare("workflow_path", self.workflow_path == packet.workflow_path);
        compare("workflow_blob", self.workflow_blob == packet.workflow_blob);
        compare(
            "workflow_commit",
            self.workflow_commit == packet.workflow_commit,
        );
        compare("plan_schema", self.plan_schema == packet.plan_schema);
        compare("plan_digest", self.plan_digest == packet.plan_digest);
        compare(
            "transaction_kind",
            self.transaction_kind == packet.transaction_kind,
        );
        compare(
            "transaction_schema",
            self.transaction_schema == packet.transaction_schema,
        );
        compare(
            "transaction_tool",
            self.transaction_tool == packet.transaction_tool,
        );
        compare(
            "transaction_request_id",
            self.transaction_request_id == packet.transaction_request_id,
        );
        compare("event_ref", self.event_ref == packet.event_ref);
        for kind in EvidenceKind::ALL {
            compare(
                &format!("evidence.{}", kind.as_str()),
                self.evidence.get(kind) == packet.evidence.get(kind),
            );
        }
        compare("subjects", self.subjects == packet.subjects);
        fields
    }
}

/// A canonical revocation: the only way, besides supersession, that a later
/// record invalidates a packet. Closure, labels, reactions and prose do not.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Revocation {
    revocation_id: String,
    repository_id: u64,
    repository_name: String,
    programme_issue: u64,
    record_issue: u64,
    issuer_id: u64,
    revokes: String,
    created_at: u64,
}

impl Revocation {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let value = parse_canonical(bytes)?;
        let mut root = Obj::new(&value, "$")?;
        if root.str("schema")? != REVOCATION_SCHEMA {
            return Err("schema: unsupported revocation schema".to_string());
        }
        let revocation_id = root.str("revocation_id")?.to_string();
        let mut repository = root.obj("repository")?;
        let repository_id = repository.id("id")?;
        let repository_name = repository.str("name")?.to_string();
        repository.finish()?;
        let programme_issue = root.id("programme_issue")?;
        let mut record = root.obj("record")?;
        let record_issue = record.id("issue")?;
        if record.str("purpose")? != REVOCATION_PURPOSE {
            return Err(format!("record.purpose: must be `{REVOCATION_PURPOSE}`"));
        }
        record.finish()?;
        let mut issuer = root.obj("issuer")?;
        let issuer_id = issuer.id("user_id")?;
        issuer.str("login")?;
        issuer.finish()?;
        let revokes = root.str("revokes")?.to_string();
        let nonce = root.str("nonce")?;
        let created_at = parse_utc_seconds(root.str("created_at")?)
            .ok_or("created_at: expected YYYY-MM-DDTHH:MM:SSZ")?;
        root.finish()?;
        if !is_digest(&revokes) || nonce.len() != 32 {
            return Err("revokes must be an authorization id and nonce 32 characters".to_string());
        }
        if revocation_id != identity_of(&value, "revocation_id")? {
            return Err("revocation_id does not match the canonical record".to_string());
        }
        Ok(Self {
            revocation_id,
            repository_id,
            repository_name,
            programme_issue,
            record_issue,
            issuer_id,
            revokes,
            created_at,
        })
    }
}

/// Copy-ready comment body for a revocation draft (the record without
/// `revocation_id`). Like a packet, it is rendered here and posted elsewhere.
pub(crate) fn render_revocation(draft: &[u8]) -> Result<(String, String), String> {
    let (id, canonical) = super::release_authorization_packet::seal_draft(draft, "revocation_id")?;
    let revocation = Revocation::parse(&canonical)?;
    let comment = format!(
        "{REVOCATION_MARKER}\n```json\n{}\n```\n\nRevokes release authorization `{}`. Operations \
         not yet performed under it stop; a completed public object is not undone by this \
         record.\n",
        String::from_utf8_lossy(&canonical),
        revocation.revokes
    );
    Ok((id, comment))
}

/// The one marked canonical line of a comment body.
///
/// The body must hold exactly one marker of this family, immediately followed
/// by a `json` fence with a single line, and must name the schema once.
pub(crate) fn extract_marked_block<'a>(
    body: &'a str,
    marker: &str,
    schema: &str,
) -> Result<&'a str, String> {
    let lines = body
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect::<Vec<_>>();
    if body.matches(MARKER_FAMILY).count() != 1 {
        return Err(format!(
            "expected exactly one release-authorization marker, found {}",
            body.matches(MARKER_FAMILY).count()
        ));
    }
    let at = lines
        .iter()
        .position(|line| *line == marker)
        .ok_or("the marker is not alone on its line or is not the expected version")?;
    let block = match lines.get(at + 1..at + 4) {
        Some(["```json", block, "```"]) if !block.is_empty() => *block,
        _ => return Err("the marker is not followed by one single-line json fence".to_string()),
    };
    // `schema` is `<family>/<version>`. No other version of this record family
    // may appear anywhere else in the body.
    let family = schema.rsplit_once('/').map_or(schema, |(family, _)| family);
    if body.matches(&format!("{family}/")).count() != 1 {
        return Err("the body names the record schema outside the one marked block".to_string());
    }
    Ok(block)
}

struct Stop(Disposition);

struct Run {
    checks: Vec<(&'static str, &'static str, String)>,
    reasons: Vec<String>,
}

impl Run {
    fn pass(&mut self, id: &'static str, detail: impl Into<String>) {
        self.checks.push((id, "passed", detail.into()));
    }

    fn skip(&mut self, id: &'static str, detail: &str) {
        self.checks.push((id, "not_applicable", detail.to_string()));
    }

    fn reject(&mut self, id: &'static str, disposition: Disposition, detail: String) -> Stop {
        self.reasons.push(format!("{id}: {detail}"));
        self.checks.push((id, "rejected", detail));
        Stop(disposition)
    }

    fn unavailable(&mut self, id: &'static str, cause: &Unavailable) -> Stop {
        let detail = format!("{}: {}", cause.kind, cause.detail);
        self.reasons.push(format!("{id}: {detail}"));
        self.checks.push((id, "unavailable", detail));
        Stop(Disposition::Unavailable)
    }
}

#[derive(Default)]
struct Observed {
    repository: Option<(u64, String)>,
    comment: Option<CommentObservation>,
    packet: Option<Packet>,
    scanned_comments: Option<usize>,
    superseded_by: Option<String>,
    revoked_by: Option<String>,
    ignored: Vec<(u64, String)>,
}

pub(crate) struct Receipt {
    pub(crate) disposition: Disposition,
    pub(crate) reasons: Vec<String>,
    /// True only for a live GitHub read; a replayed fixture is not authority.
    pub(crate) live: bool,
    pub(crate) authorization_id: Option<String>,
    value: Json,
}

impl Receipt {
    /// The record is authorized and was read from GitHub, not replayed.
    pub(crate) fn admits(&self) -> bool {
        self.disposition == Disposition::Authorized && self.live
    }

    pub(crate) fn json(&self) -> String {
        let mut text = serde_json::to_string_pretty(&self.value).unwrap_or_default();
        text.push('\n');
        text
    }

    pub(crate) fn receipt_id(&self) -> &str {
        self.value
            .get("receipt_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
    }

    pub(crate) fn markdown(&self) -> String {
        let field = |pointer: &str| {
            self.value
                .pointer(pointer)
                .map(|value| match value {
                    Json::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "null".to_string())
        };
        let mut out = format!(
            "# Release authorization receipt\n\n- Disposition: `{}`\n- Authorization: `{}`\n\
             - Record: `{}` issue {} comment {}\n- Body SHA-256: `{}`\n- Record source: `{}`\n\
             - Receipt: `{}`\n- External mutations attempted: none\n- Credentials requested: none\n\n\
             | Check | State | Detail |\n| --- | --- | --- |\n",
            self.disposition.as_str(),
            self.authorization_id
                .as_deref()
                .unwrap_or("not established"),
            field("/locator/repository_name"),
            field("/locator/issue"),
            field("/locator/comment_id"),
            field("/locator/body_sha256"),
            field("/record_source"),
            self.receipt_id(),
        );
        for check in self
            .value
            .get("checks")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
        {
            let cell = |key: &str| {
                check
                    .get(key)
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .replace('|', "\\|")
            };
            out.push_str(&format!(
                "| `{}` | {} | {} |\n",
                cell("id"),
                cell("state"),
                cell("detail")
            ));
        }
        out.push_str(
            "\nThis receipt establishes that one exact record was read and evaluated. It does \
             not bind the record to an event, reserve or consume it, create a tag, request a \
             credential or publish anything.\n",
        );
        out
    }
}

pub(crate) fn verify(
    reader: &dyn RecordReader,
    policy: &AuthorizerPolicy,
    locator: &Locator,
    bindings: &Bindings,
    now: u64,
) -> Receipt {
    let mut run = Run {
        checks: Vec::new(),
        reasons: Vec::new(),
    };
    let mut observed = Observed::default();
    let disposition = match evaluate(
        &mut run,
        &mut observed,
        reader,
        policy,
        locator,
        bindings,
        now,
    ) {
        Ok(()) => Disposition::Authorized,
        Err(Stop(disposition)) => disposition,
    };
    build_receipt(
        run,
        observed,
        reader.source(),
        policy,
        locator,
        now,
        disposition,
    )
}

fn evaluate(
    run: &mut Run,
    observed: &mut Observed,
    reader: &dyn RecordReader,
    policy: &AuthorizerPolicy,
    locator: &Locator,
    bindings: &Bindings,
    now: u64,
) -> Result<(), Stop> {
    use Disposition as D;
    let repository = locator.repository_name.as_str();
    let hex = |text: &str| text.len() == 64 && is_digest(&format!("sha256:{text}"));
    if !valid_repository_name(repository) || !hex(&locator.body_sha256) {
        return Err(run.reject(
            "locator",
            D::Mismatched,
            "the locator needs an owner/name repository and a 64-hex body digest".to_string(),
        ));
    }
    run.pass(
        "locator",
        "exact repository, issue, comment and body digest supplied",
    );

    let seen = reader
        .repository(repository)
        .map_err(|cause| run.unavailable("repository_identity", &cause))?;
    observed.repository = Some((seen.id, seen.full_name.clone()));
    let expected = [
        (locator.repository_id, repository),
        (policy.repository_id, policy.repository_name.as_str()),
    ];
    if expected
        .iter()
        .any(|(id, name)| *id != seen.id || *name != seen.full_name)
    {
        return Err(run.reject(
            "repository_identity",
            D::Mismatched,
            format!(
                "observed repository {} ({}) is not the located, policy-governed repository",
                seen.full_name, seen.id
            ),
        ));
    }
    run.pass(
        "repository_identity",
        "repository id and name match locator and policy",
    );

    let comment = reader
        .comment(repository, locator.comment_id)
        .map_err(|cause| run.unavailable("record_location", &cause))?;
    observed.comment = Some(comment.clone());
    if comment.id != locator.comment_id || comment.issue != locator.issue {
        return Err(run.reject(
            "record_location",
            D::Mismatched,
            format!(
                "observed comment {} on issue {}, not the located record",
                comment.id, comment.issue
            ),
        ));
    }
    run.pass("record_location", "comment id and issue match the locator");

    if sha256_hex(comment.body.as_bytes()) != locator.body_sha256 {
        return Err(run.reject(
            "record_body_digest",
            D::Mismatched,
            "the fetched body does not have the expected SHA-256".to_string(),
        ));
    }
    run.pass(
        "record_body_digest",
        "fetched body bytes match the expected SHA-256",
    );

    if parse_utc_seconds(&comment.created_at).is_none() || comment.created_at != comment.updated_at
    {
        return Err(run.reject(
            "record_unedited",
            D::Mismatched,
            "the comment has been edited since it was created".to_string(),
        ));
    }
    run.pass(
        "record_unedited",
        "created and updated observations are identical",
    );

    if let Some(kind) = secret_shaped(&comment.body) {
        // The value is deliberately not echoed anywhere in the receipt.
        return Err(run.reject(
            "secret_scan",
            D::Malformed,
            format!("the record contains a credential-shaped value ({kind}); value withheld"),
        ));
    }
    run.pass("secret_scan", "no credential-shaped value in the record");

    let block = extract_marked_block(&comment.body, RECORD_MARKER, PACKET_SCHEMA)
        .map_err(|error| run.reject("packet_block", D::Malformed, error))?;
    run.pass("packet_block", "exactly one marked packet block");
    let value = parse_canonical(block.as_bytes())
        .map_err(|error| run.reject("packet_canonical_bytes", D::Malformed, error))?;
    run.pass(
        "packet_canonical_bytes",
        "block bytes are the canonical form",
    );
    let packet = Packet::read_checked(&value)
        .map_err(|error| run.reject("packet_fields_and_profile", D::Malformed, error))?;
    run.pass(
        "packet_fields_and_profile",
        "closed fields satisfy the profile matrix",
    );
    packet
        .check_identity(&value)
        .map_err(|error| run.reject("authorization_id", D::Malformed, error))?;
    run.pass("authorization_id", "recomputed from the canonical packet");
    observed.packet = Some(packet.clone());

    if packet.repository_id != seen.id
        || packet.repository_name != seen.full_name
        || packet.record_issue != comment.issue
    {
        return Err(run.reject(
            "record_agreement",
            D::Mismatched,
            "the packet names a different repository or record issue than where it was found"
                .to_string(),
        ));
    }
    run.pass(
        "record_agreement",
        "packet repository and record issue match the record",
    );

    if packet.issuer_id != comment.author_id || packet.issuer_login != comment.author_login {
        return Err(run.reject(
            "issuer_identity",
            D::Denied,
            format!(
                "the comment author {} ({}) is not the packet issuer",
                comment.author_login, comment.author_id
            ),
        ));
    }
    run.pass(
        "issuer_identity",
        "comment author id and login equal the packet issuer",
    );

    check_authorizer(policy, &comment, packet.profile, now)
        .map_err(|error| run.reject("authorizer_policy", D::Denied, error))?;
    run.pass(
        "authorizer_policy",
        "author is a current policy authorizer for this profile",
    );

    let mismatches = bindings.mismatches(&packet);
    if !mismatches.is_empty() {
        return Err(run.reject(
            "bindings",
            D::Mismatched,
            format!(
                "packet differs from the supplied inputs at: {}",
                mismatches.join(", ")
            ),
        ));
    }
    run.pass(
        "bindings",
        "every packet identity equals the supplied exact input",
    );

    match &packet.policy_ruling {
        None => run.skip("policy_ruling", "this profile cites no policy ruling"),
        Some(ruling) => {
            let cited = reader
                .comment(repository, ruling.comment_id)
                .map_err(|cause| run.unavailable("policy_ruling", &cause))?;
            let exact = cited.id == ruling.comment_id
                && cited.issue == ruling.issue
                && digest(cited.body.as_bytes()) == ruling.body_sha256;
            if !exact {
                return Err(run.reject(
                    "policy_ruling",
                    D::Mismatched,
                    "the cited ruling comment is not the exact located bytes".to_string(),
                ));
            }
            if policy.authorizer(cited.author_id).is_none() {
                return Err(run.reject(
                    "policy_ruling",
                    D::Denied,
                    "the cited ruling was not written by a policy authorizer".to_string(),
                ));
            }
            run.pass(
                "policy_ruling",
                "cited ruling bytes and author re-read; prose not parsed",
            );
        }
    }

    if now < packet.not_before {
        return Err(run.reject(
            "validity_window",
            D::Denied,
            "the packet is not yet valid".to_string(),
        ));
    }
    if now >= packet.not_after {
        return Err(run.reject(
            "validity_window",
            D::Expired,
            "the packet validity window has elapsed".to_string(),
        ));
    }
    run.pass(
        "validity_window",
        "evaluation time is inside the validity window",
    );

    let index = reader
        .issue_comments(repository, packet.record_issue)
        .map_err(|cause| run.unavailable("supersession", &cause))?;
    if !index.contains(&comment) {
        return Err(run.unavailable(
            "supersession",
            &Unavailable::new(
                "inconsistent_index",
                "the comment index does not contain the located record as fetched",
            ),
        ));
    }
    observed.scanned_comments = Some(index.len());
    scan_later_records(observed, policy, &packet, &comment, &index, now);
    if let Some(id) = &observed.revoked_by {
        return Err(run.reject("supersession", D::Revoked, format!("revoked by {id}")));
    }
    if let Some(id) = &observed.superseded_by {
        return Err(run.reject("supersession", D::Superseded, format!("superseded by {id}")));
    }
    run.pass(
        "supersession",
        "no authorized supersession or revocation in the bounded index",
    );
    Ok(())
}

fn check_authorizer(
    policy: &AuthorizerPolicy,
    comment: &CommentObservation,
    profile: Profile,
    now: u64,
) -> Result<(), String> {
    if now >= policy.review_deadline {
        return Err("the authorizer policy is past its review_after date".to_string());
    }
    let authorizer = policy.authorizer(comment.author_id).ok_or_else(|| {
        format!(
            "user id {} is not in the authorizer policy",
            comment.author_id
        )
    })?;
    if authorizer.login != comment.author_login {
        return Err(format!(
            "user id {} now has login {}, not the policy's {}",
            comment.author_id, comment.author_login, authorizer.login
        ));
    }
    if !authorizer.profiles.contains(&profile) {
        return Err(format!(
            "{} is not an authorizer for profile {}",
            authorizer.login,
            profile.as_str()
        ));
    }
    Ok(())
}

/// Finds an authorized record that supersedes or revokes `packet`. A record
/// that names this packet but is not itself a valid, unedited, authorized
/// record is listed as ignored and changes nothing.
fn scan_later_records(
    observed: &mut Observed,
    policy: &AuthorizerPolicy,
    packet: &Packet,
    own: &CommentObservation,
    index: &[CommentObservation],
    now: u64,
) {
    let mut superseders = Vec::new();
    let mut revokers = Vec::new();
    for other in index.iter().filter(|other| other.id != own.id) {
        if !other.body.contains(MARKER_FAMILY) {
            continue;
        }
        let authorized_author = |profile: Profile| {
            if other.created_at != other.updated_at {
                return Err("the record was edited".to_string());
            }
            check_authorizer(policy, other, profile, now)
        };
        if let Ok(block) = extract_marked_block(&other.body, REVOCATION_MARKER, REVOCATION_SCHEMA) {
            let outcome = Revocation::parse(block.as_bytes()).and_then(|revocation| {
                if revocation.revokes != packet.authorization_id {
                    return Ok(None);
                }
                authorized_author(packet.profile)?;
                let same_boundary = revocation.repository_id == packet.repository_id
                    && revocation.repository_name == packet.repository_name
                    && revocation.programme_issue == packet.programme_issue
                    && revocation.record_issue == packet.record_issue
                    && revocation.issuer_id == other.author_id
                    && revocation.created_at >= packet.created_at;
                if same_boundary {
                    Ok(Some(revocation.revocation_id))
                } else {
                    Err("the revocation names a different boundary or issuer".to_string())
                }
            });
            match outcome {
                Ok(Some(id)) => revokers.push(id),
                Ok(None) => {}
                Err(reason) => observed.ignored.push((other.id, reason)),
            }
            continue;
        }
        let outcome = extract_marked_block(&other.body, RECORD_MARKER, PACKET_SCHEMA)
            .and_then(|block| Packet::parse(block.as_bytes()))
            .and_then(|later| {
                if later.supersedes.as_deref() != Some(packet.authorization_id.as_str()) {
                    return Ok(None);
                }
                authorized_author(later.profile)?;
                let same_boundary = later.repository_id == packet.repository_id
                    && later.repository_name == packet.repository_name
                    && later.programme_issue == packet.programme_issue
                    && later.record_issue == packet.record_issue
                    && later.profile == packet.profile
                    && later.transaction_kind == packet.transaction_kind
                    && later.issuer_id == other.author_id
                    && later.sequence > packet.sequence;
                if same_boundary {
                    Ok(Some(later.authorization_id))
                } else {
                    Err("the superseding packet is outside this packet's boundary".to_string())
                }
            });
        match outcome {
            Ok(Some(id)) => superseders.push(id),
            Ok(None) => {}
            // A record that does not parse cannot be shown to name this packet;
            // it is reported only when it mentions this authorization id.
            Err(reason) if other.body.contains(&packet.authorization_id) => {
                observed.ignored.push((other.id, reason));
            }
            Err(_) => {}
        }
    }
    // Enumeration order of the index must not change the receipt.
    superseders.sort();
    revokers.sort();
    observed.ignored.sort();
    observed.superseded_by = superseders.into_iter().next();
    observed.revoked_by = revokers.into_iter().next();
}

fn packet_json(packet: &Packet) -> Json {
    let names = |operations: Vec<Operation>| {
        let mut names = operations
            .into_iter()
            .map(Operation::as_str)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    };
    let allowed = packet.allowed.iter().copied().collect::<Vec<_>>();
    let denied = Operation::ALL
        .iter()
        .copied()
        .filter(|operation| !packet.allowed.contains(operation))
        .collect::<Vec<_>>();
    json!({
        "profile": packet.profile.as_str(),
        "programme_issue": packet.programme_issue,
        "issuer": {"user_id": packet.issuer_id, "login": packet.issuer_login},
        "sequence": packet.sequence,
        "nonce": packet.nonce,
        "validity_unix": {
            "created_at": packet.created_at,
            "not_before": packet.not_before,
            "not_after": packet.not_after,
        },
        "supersedes": packet.supersedes,
        "version": packet.version,
        "source": {"commit": packet.source_commit, "tree": packet.source_tree},
        "evidence": EvidenceKind::ALL
            .iter()
            .map(|kind| (kind.as_str().to_string(), json!(packet.evidence.get(kind).cloned().flatten())))
            .collect::<serde_json::Map<_, _>>(),
        "policy_ruling": packet.policy_ruling.as_ref().map(|ruling| json!({
            "issue": ruling.issue,
            "comment_id": ruling.comment_id,
            "body_sha256": ruling.body_sha256,
        })),
        "workflow": {
            "path": packet.workflow_path,
            "blob": packet.workflow_blob,
            "commit": packet.workflow_commit,
            "plan_schema": packet.plan_schema,
            "plan_digest": packet.plan_digest,
        },
        "transaction": {
            "kind": packet.transaction_kind.as_str(),
            "schema": packet.transaction_schema,
            "tool": packet.transaction_tool,
            "request_id": packet.transaction_request_id,
        },
        "event": {"kind": packet.event_kind.as_str(), "ref": packet.event_ref},
        "allowed_operations": names(allowed),
        "denied_operations": names(denied),
        "channels": Channel::ALL
            .iter()
            .map(|channel| {
                let authorized = packet.channels.get(channel).copied().unwrap_or(false);
                (channel.as_str().to_string(), json!(authorized))
            })
            .collect::<serde_json::Map<_, _>>(),
        "subjects": packet.subjects.iter().map(|subject| json!({
            "class": subject.class.as_str(),
            "name": subject.name,
            "sha256": subject.sha256,
        })).collect::<Vec<_>>(),
        // Identifier names only. No environment or credential value exists here.
        "environments": packet.environments.iter().map(|environment| json!({
            "channel": environment.channel.as_str(),
            "name": environment.name,
            "credential_names": environment.credentials,
        })).collect::<Vec<_>>(),
        "executors": packet.executors.iter().map(|(kind, id)| json!({"kind": kind, "id": id})).collect::<Vec<_>>(),
        "consumption": {"mode": "one_use", "idempotency": packet.idempotency},
    })
}

fn build_receipt(
    run: Run,
    observed: Observed,
    source: &'static str,
    policy: &AuthorizerPolicy,
    locator: &Locator,
    now: u64,
    disposition: Disposition,
) -> Receipt {
    let checks = CHECKS
        .iter()
        .map(|id| {
            let (state, detail) = run
                .checks
                .iter()
                .find(|(seen, _, _)| seen == id)
                .map_or(("not_evaluated", ""), |(_, state, detail)| {
                    (*state, detail.as_str())
                });
            json!({"id": id, "state": state, "detail": detail})
        })
        .collect::<Vec<_>>();
    let live = source == "github_api";
    let authorization_id = observed
        .packet
        .as_ref()
        .map(|packet| packet.authorization_id.clone());
    let mut value = json!({
        "schema": RECEIPT_SCHEMA,
        "record_source": source,
        "live_authority": live,
        "evaluated_at_unix": now,
        "locator": {
            "repository_id": locator.repository_id,
            "repository_name": locator.repository_name,
            "issue": locator.issue,
            "comment_id": locator.comment_id,
            "body_sha256": locator.body_sha256,
        },
        "observations": {
            "repository": observed.repository.as_ref().map(|(id, name)| json!({"id": id, "full_name": name})),
            "comment": observed.comment.as_ref().map(|comment| json!({
                "id": comment.id,
                "issue": comment.issue,
                "author_id": comment.author_id,
                "author_login": comment.author_login,
                "created_at": comment.created_at,
                "updated_at": comment.updated_at,
                "body_sha256": sha256_hex(comment.body.as_bytes()),
            })),
            "policy": {
                "path": POLICY_PATH,
                "schema": POLICY_SCHEMA,
                "digest": policy.digest,
                "review_deadline_unix": policy.review_deadline,
            },
        },
        "authorization_id": authorization_id,
        "packet": observed.packet.as_ref().map(packet_json),
        "supersession": {
            "scanned_comments": observed.scanned_comments,
            "superseded_by": observed.superseded_by,
            "revoked_by": observed.revoked_by,
            "ignored_candidates": observed.ignored.iter().map(|(comment_id, reason)| json!({
                "comment_id": comment_id,
                "reason": reason,
            })).collect::<Vec<_>>(),
        },
        "checks": checks,
        "external_mutation_attempts": false,
        "credential_requests": false,
        "disposition": disposition.as_str(),
        "reasons": run.reasons,
        "invalidations": INVALIDATIONS,
        "non_claims": NON_CLAIMS,
    });
    // The identity is over the canonical form, so key order and pretty
    // printing are not part of it.
    let canonical = crate::exact_schema_json::parse(value.to_string().as_bytes())
        .map(|exact| exact.to_string())
        .unwrap_or_default();
    value["receipt_id"] = json!(digest(canonical.as_bytes()));
    Receipt {
        disposition,
        reasons: value["reasons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect(),
        live,
        authorization_id,
        value,
    }
}

#[cfg(test)]
mod tests;
