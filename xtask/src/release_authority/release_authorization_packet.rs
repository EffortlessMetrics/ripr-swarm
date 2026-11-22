//! `ReleaseAuthorizationPacket` v1: one canonical, profile-governed object
//! (ripr#1651).
//!
//! Every field is always present. A field a profile does not use is an
//! explicit `null`, an empty array or `not_authorized`; nothing is defaulted.
//! `allowed_operations` and `denied_operations` together name the whole closed
//! operation set, so an operation is never denied or allowed by omission.

use std::collections::{BTreeMap, BTreeSet};

use super::{digest, parse_canonical, parse_plain_u64, parse_utc_seconds};
use crate::exact_schema_json::Value;

pub(crate) const PACKET_SCHEMA: &str = "ripr-release-authorization-packet/v1";
pub(crate) const PACKET_TOOL: &str = "xtask-release-authority/1";
pub(crate) const RECORD_MARKER: &str = "<!-- ripr-release-authorization:v1 -->";
pub(crate) const RECORD_PURPOSE: &str = "owner_release_authorization";
pub(crate) const CONSUMPTION_RECEIPT_SCHEMA: &str = "ripr-release-authorization-consumption/v1";
const PROBE_VERSION_PREFIX: &str = "0.0.0-ripr-trigger-probe.";
const PROBE_MAX_WINDOW_SECONDS: u64 = 24 * 3_600;
const MAX_WINDOW_SECONDS: u64 = 30 * 24 * 3_600;

macro_rules! closed_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub(crate) enum $name { $($variant),+ }

        impl $name {
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub(crate) fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            pub(crate) fn parse(text: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|value| value.as_str() == text)
            }
        }
    };
}

closed_enum!(Profile {
    ReservedProbe => "reserved_probe",
    PublicRc => "public_rc",
    StableCore => "stable_core",
    StableChannelContinuation => "stable_channel_continuation",
});

closed_enum!(TransactionKind {
    Github => "github",
    CratesIo => "crates_io",
    VsMarketplace => "vs_marketplace",
    OpenVsx => "open_vsx",
    Npm => "npm",
    Probe => "probe",
});

closed_enum!(EventKind {
    AnnotatedTagPush => "annotated_tag_push",
    ExactChannelContinuation => "exact_channel_continuation",
});

closed_enum!(Operation {
    CreateAnnotatedTag => "create_annotated_tag",
    TriggerNoPublishPlan => "trigger_no_publish_plan",
    ReadEventEvidence => "read_event_artifact_attestation",
    RecordConsumptionReceipt => "record_consumption_receipt",
    DeleteExactProbeTag => "delete_exact_probe_tag",
    GithubPrereleaseCreate => "github_prerelease_create",
    GithubReleaseCreate => "github_release_create",
    GithubAssetUpload => "github_asset_upload",
    ProvenanceAttest => "provenance_attest",
    RequestNamedEnvironment => "request_named_environment",
    CratesIoPublish => "crates_io_publish",
    VsMarketplacePublish => "vs_marketplace_publish",
    OpenVsxPublish => "open_vsx_publish",
    NpmPublish => "npm_publish",
    BranchOrExistingRefUpdate => "branch_or_existing_ref_update",
    SigningKey => "signing_key_operation",
});

closed_enum!(Channel {
    GitTag => "git_tag",
    GithubPrerelease => "github_prerelease",
    GithubRelease => "github_release",
    CratesIo => "crates_io",
    VsMarketplace => "vs_marketplace",
    OpenVsx => "open_vsx",
    Npm => "npm",
});

closed_enum!(SubjectClass {
    ServerArchive => "server_archive",
    ServerManifest => "server_manifest",
    Checksums => "checksums",
    Vsix => "vsix",
    AttestationBundle => "attestation_bundle",
    Crate => "crate",
});

/// Evidence digests a packet may bind. Which are required is profile-governed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EvidenceKind {
    ShipPacket,
    Readiness,
    Qualification,
    PublicRcVerification,
    Cohort,
    StableDecision,
    PredecessorReceipt,
}

impl EvidenceKind {
    pub(crate) const ALL: &'static [Self] = &[
        Self::ShipPacket,
        Self::Readiness,
        Self::Qualification,
        Self::PublicRcVerification,
        Self::Cohort,
        Self::StableDecision,
        Self::PredecessorReceipt,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ShipPacket => "ship_packet",
            Self::Readiness => "readiness",
            Self::Qualification => "qualification",
            Self::PublicRcVerification => "public_rc_verification",
            Self::Cohort => "cohort",
            Self::StableDecision => "stable_decision",
            Self::PredecessorReceipt => "predecessor_receipt",
        }
    }
}

/// The three closed statement lists every v1 packet carries verbatim.
pub(crate) const INVALIDATIONS: &[&str] = &[
    "authorizer_removed_from_policy",
    "candidate_or_tree_moved",
    "evidence_digest_changed",
    "record_edited_or_deleted",
    "superseded_or_revoked",
    "validity_window_elapsed",
    "workflow_or_plan_changed",
];
pub(crate) const STOP_CONDITIONS: &[&str] = &[
    "any_check_rejected",
    "any_check_unavailable",
    "conflicting_public_object",
];
pub(crate) const NON_CLAIMS: &[&str] = &[
    "not_a_publication",
    "not_a_reservation_or_consumption",
    "not_an_event_binding",
    "not_runtime_or_mutation_test_evidence",
];

/// Strict reader over one JSON object: a missing field and an unread field are
/// both errors, so the field set is closed in both directions.
pub(crate) struct Obj<'a> {
    path: String,
    map: &'a BTreeMap<String, Value>,
    used: BTreeSet<&'a str>,
}

impl<'a> Obj<'a> {
    pub(crate) fn new(value: &'a Value, path: &str) -> Result<Self, String> {
        let map = value
            .as_object()
            .ok_or_else(|| format!("{path}: expected an object"))?;
        Ok(Self {
            path: path.to_string(),
            map,
            used: BTreeSet::new(),
        })
    }

    fn take(&mut self, key: &str) -> Result<&'a Value, String> {
        let (name, value) = self
            .map
            .get_key_value(key)
            .ok_or_else(|| format!("{}.{key}: required field is missing", self.path))?;
        self.used.insert(name.as_str());
        Ok(value)
    }

    pub(crate) fn str(&mut self, key: &str) -> Result<&'a str, String> {
        let path = format!("{}.{key}", self.path);
        self.take(key)?
            .as_str()
            .filter(|text| !text.is_empty())
            .ok_or_else(|| format!("{path}: expected a nonempty string"))
    }

    pub(crate) fn opt_str(&mut self, key: &str) -> Result<Option<&'a str>, String> {
        match self.take(key)? {
            Value::Null => Ok(None),
            Value::String(text) if !text.is_empty() => Ok(Some(text)),
            _ => Err(format!(
                "{}.{key}: expected a nonempty string or an explicit null",
                self.path
            )),
        }
    }

    pub(crate) fn id(&mut self, key: &str) -> Result<u64, String> {
        let path = format!("{}.{key}", self.path);
        self.take(key)?
            .as_number()
            .and_then(parse_plain_u64)
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{path}: expected a positive integer"))
    }

    pub(crate) fn obj(&mut self, key: &str) -> Result<Obj<'a>, String> {
        let path = format!("{}.{key}", self.path);
        Obj::new(self.take(key)?, &path)
    }

    pub(crate) fn opt_obj(&mut self, key: &str) -> Result<Option<Obj<'a>>, String> {
        let path = format!("{}.{key}", self.path);
        match self.take(key)? {
            Value::Null => Ok(None),
            value => Obj::new(value, &path).map(Some),
        }
    }

    pub(crate) fn arr(&mut self, key: &str) -> Result<&'a [Value], String> {
        let path = format!("{}.{key}", self.path);
        self.take(key)?
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| format!("{path}: expected an array"))
    }

    pub(crate) fn finish(self) -> Result<(), String> {
        match self
            .map
            .keys()
            .find(|key| !self.used.contains(key.as_str()))
        {
            Some(extra) => Err(format!("{}.{extra}: unknown field", self.path)),
            None => Ok(()),
        }
    }
}

pub(crate) fn is_git_object(text: &str) -> bool {
    text.len() == 40 && is_lower_hex(text)
}

pub(crate) fn is_digest(text: &str) -> bool {
    text.strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && is_lower_hex(hex))
}

fn is_lower_hex(text: &str) -> bool {
    text.bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_identifier(text: &str) -> bool {
    (1..=96).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
}

fn git_object(obj: &mut Obj<'_>, key: &str) -> Result<String, String> {
    let text = obj.str(key)?;
    if is_git_object(text) {
        Ok(text.to_string())
    } else {
        Err(format!(
            "{key}: expected a 40-character lowercase Git object id"
        ))
    }
}

fn digest_field(obj: &mut Obj<'_>, key: &str) -> Result<String, String> {
    let text = obj.str(key)?;
    if is_digest(text) {
        Ok(text.to_string())
    } else {
        Err(format!("{key}: expected `sha256:` and 64 lowercase hex"))
    }
}

fn identifier(obj: &mut Obj<'_>, key: &str) -> Result<String, String> {
    let text = obj.str(key)?;
    if is_identifier(text) {
        Ok(text.to_string())
    } else {
        Err(format!("{key}: expected an identifier of at most 96 bytes"))
    }
}

fn closed<T: Copy>(
    obj: &mut Obj<'_>,
    key: &str,
    parse: fn(&str) -> Option<T>,
) -> Result<T, String> {
    let text = obj.str(key)?;
    parse(text).ok_or_else(|| format!("{key}: `{text}` is not a known value"))
}

/// A string array that must already be strictly ascending: sorted, no repeats.
fn ascending_strings<'a>(items: &'a [Value], path: &str) -> Result<Vec<&'a str>, String> {
    let texts = items
        .iter()
        .map(|item| {
            item.as_str()
                .ok_or_else(|| format!("{path}: expected strings"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if texts.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(format!(
            "{path}: entries must be unique and in ascending order"
        ));
    }
    Ok(texts)
}

fn fixed_statements(obj: &mut Obj<'_>, key: &str, expected: &[&str]) -> Result<(), String> {
    if ascending_strings(obj.arr(key)?, key)? == expected {
        Ok(())
    } else {
        Err(format!("{key}: must be exactly the v1 statement list"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordRef {
    pub(crate) issue: u64,
    pub(crate) comment_id: u64,
    pub(crate) body_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Subject {
    pub(crate) class: SubjectClass,
    pub(crate) name: String,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Environment {
    pub(crate) channel: Channel,
    pub(crate) name: String,
    pub(crate) credentials: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Packet {
    pub(crate) authorization_id: String,
    pub(crate) repository_id: u64,
    pub(crate) repository_name: String,
    pub(crate) programme_issue: u64,
    pub(crate) record_issue: u64,
    pub(crate) profile: Profile,
    pub(crate) issuer_id: u64,
    pub(crate) issuer_login: String,
    pub(crate) sequence: u64,
    pub(crate) nonce: String,
    pub(crate) created_at: u64,
    pub(crate) not_before: u64,
    pub(crate) not_after: u64,
    pub(crate) supersedes: Option<String>,
    pub(crate) version: String,
    pub(crate) source_commit: String,
    pub(crate) source_tree: String,
    pub(crate) evidence: BTreeMap<EvidenceKind, Option<String>>,
    pub(crate) policy_ruling: Option<RecordRef>,
    pub(crate) workflow_path: String,
    pub(crate) workflow_blob: String,
    pub(crate) workflow_commit: String,
    pub(crate) plan_schema: String,
    pub(crate) plan_digest: String,
    pub(crate) transaction_kind: TransactionKind,
    pub(crate) transaction_schema: String,
    pub(crate) transaction_tool: String,
    pub(crate) transaction_request_id: String,
    pub(crate) event_kind: EventKind,
    pub(crate) event_ref: Option<String>,
    pub(crate) allowed: BTreeSet<Operation>,
    pub(crate) channels: BTreeMap<Channel, bool>,
    pub(crate) subjects: Vec<Subject>,
    pub(crate) environments: Vec<Environment>,
    pub(crate) executors: Vec<(String, u64)>,
    pub(crate) idempotency: String,
}

pub(crate) struct Rendered {
    pub(crate) authorization_id: String,
    pub(crate) canonical: Vec<u8>,
    pub(crate) comment: String,
}

/// `sha256` over the canonical object with its own id field removed.
pub(crate) fn identity_of(object: &Value, id_field: &str) -> Result<String, String> {
    let mut map = object
        .as_object()
        .ok_or("$: expected an object")?
        .to_owned();
    map.remove(id_field);
    Ok(digest(Value::Object(map).to_string().as_bytes()))
}

pub(crate) fn authorization_id_of(packet: &Value) -> Result<String, String> {
    identity_of(packet, "authorization_id")
}

/// Canonicalizes a draft that omits its id field and inserts the computed id.
pub(crate) fn seal_draft(draft: &[u8], id_field: &str) -> Result<(String, Vec<u8>), String> {
    let value = crate::exact_schema_json::parse(draft)
        .map_err(|e| format!("draft is not one JSON document: {e}"))?;
    let mut map = value
        .as_object()
        .ok_or("draft must be a JSON object")?
        .to_owned();
    if map.contains_key(id_field) {
        return Err(format!(
            "draft must not carry {id_field}; it is computed from the canonical bytes"
        ));
    }
    let id = identity_of(&Value::Object(map.clone()), id_field)?;
    map.insert(id_field.to_string(), Value::String(id.clone()));
    Ok((id, Value::Object(map).to_string().into_bytes()))
}

/// Canonicalizes a draft (a packet without `authorization_id`), validates it
/// against the profile matrix and renders the copy-ready comment body.
pub(crate) fn render_from_draft(draft: &[u8]) -> Result<Rendered, String> {
    let (authorization_id, canonical) = seal_draft(draft, "authorization_id")?;
    let packet = Packet::parse(&canonical)?;
    let comment = render_comment(&packet, &canonical);
    Ok(Rendered {
        authorization_id,
        canonical,
        comment,
    })
}

/// The comment body: the marker, one fenced canonical line, then bounded
/// context. The verifier reads only the marked block; the prose is for people.
pub(crate) fn render_comment(packet: &Packet, canonical: &[u8]) -> String {
    let authorized = packet
        .channels
        .iter()
        .filter(|(_, authorized)| **authorized)
        .map(|(channel, _)| channel.as_str())
        .collect::<Vec<_>>();
    format!(
        "{RECORD_MARKER}\n```json\n{}\n```\n\nRelease authorization `{}` for profile `{}`, \
         version `{}`, source commit `{}`. Authorized channels: {}. The marked block above is \
         the only authority in this comment; this paragraph is context and is never parsed. \
         Editing this comment invalidates the authorization.\n",
        String::from_utf8_lossy(canonical),
        packet.authorization_id,
        packet.profile.as_str(),
        packet.version,
        packet.source_commit,
        if authorized.is_empty() {
            "none".to_string()
        } else {
            authorized.join(", ")
        },
    )
}

impl Packet {
    /// Canonical bytes, closed fields, profile matrix and recomputed identity.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let value = parse_canonical(bytes)?;
        let packet = Self::read_checked(&value)?;
        packet.check_identity(&value)?;
        Ok(packet)
    }

    /// Closed fields and the profile matrix of an already canonical value.
    pub(crate) fn read_checked(value: &Value) -> Result<Self, String> {
        no_secret_shaped_text(value)?;
        let packet = Self::read(value)?;
        packet.check_profile()?;
        Ok(packet)
    }

    pub(crate) fn check_identity(&self, value: &Value) -> Result<(), String> {
        let recomputed = authorization_id_of(value)?;
        if self.authorization_id == recomputed {
            Ok(())
        } else {
            Err(format!(
                "authorization_id does not match the canonical packet (recomputed {recomputed})"
            ))
        }
    }

    fn read(value: &Value) -> Result<Self, String> {
        let mut root = Obj::new(value, "$")?;
        if root.str("schema")? != PACKET_SCHEMA {
            return Err(format!(
                "schema: unsupported; this verifier reads only {PACKET_SCHEMA}"
            ));
        }
        if root.str("tool")? != PACKET_TOOL {
            return Err(format!(
                "tool: unsupported; this verifier reads only {PACKET_TOOL}"
            ));
        }
        let authorization_id = digest_field(&mut root, "authorization_id")?;

        let mut repository = root.obj("repository")?;
        let repository_id = repository.id("id")?;
        let repository_name = repository.str("name")?.to_string();
        repository.finish()?;
        let programme_issue = root.id("programme_issue")?;

        let mut record = root.obj("record")?;
        let record_issue = record.id("issue")?;
        if record.str("purpose")? != RECORD_PURPOSE {
            return Err(format!("record.purpose: must be `{RECORD_PURPOSE}`"));
        }
        record.finish()?;

        let profile = closed(&mut root, "profile", Profile::parse)?;
        let mut issuer = root.obj("issuer")?;
        let issuer_id = issuer.id("user_id")?;
        let issuer_login = issuer.str("login")?.to_string();
        issuer.finish()?;
        let sequence = root.id("sequence")?;
        let nonce = root.str("nonce")?.to_string();
        if nonce.len() != 32 || !is_lower_hex(&nonce) {
            return Err("nonce: expected 32 lowercase hex characters".to_string());
        }

        let mut validity = root.obj("validity")?;
        let mut instant = |key: &str| -> Result<u64, String> {
            parse_utc_seconds(validity.str(key)?)
                .ok_or_else(|| format!("validity.{key}: expected YYYY-MM-DDTHH:MM:SSZ"))
        };
        let (created_at, not_before, not_after) = (
            instant("created_at")?,
            instant("not_before")?,
            instant("not_after")?,
        );
        validity.finish()?;

        let supersedes = root.opt_str("supersedes")?.map(str::to_string);
        if supersedes.as_deref().is_some_and(|id| !is_digest(id)) {
            return Err("supersedes: expected an authorization id or null".to_string());
        }
        let version = root.str("version")?.to_string();

        let mut source = root.obj("source")?;
        let source_kind = source.str("kind")?;
        let expected_kind = if profile == Profile::ReservedProbe {
            "probe_source"
        } else {
            "source_release"
        };
        if source_kind != expected_kind {
            return Err(format!(
                "source.kind: profile {} requires `{expected_kind}`",
                profile.as_str()
            ));
        }
        let source_commit = git_object(&mut source, "commit")?;
        let source_tree = git_object(&mut source, "tree")?;
        source.finish()?;

        let mut evidence_obj = root.obj("evidence")?;
        let mut evidence = BTreeMap::new();
        for kind in EvidenceKind::ALL {
            let value = evidence_obj.opt_str(kind.as_str())?;
            if value.is_some_and(|text| !is_digest(text)) {
                return Err(format!(
                    "evidence.{}: expected a sha256 digest or null",
                    kind.as_str()
                ));
            }
            evidence.insert(*kind, value.map(str::to_string));
        }
        let policy_ruling = match evidence_obj.opt_obj("policy_ruling")? {
            None => None,
            Some(mut ruling) => {
                let parsed = RecordRef {
                    issue: ruling.id("issue")?,
                    comment_id: ruling.id("comment_id")?,
                    body_sha256: digest_field(&mut ruling, "body_sha256")?,
                };
                ruling.finish()?;
                Some(parsed)
            }
        };
        evidence_obj.finish()?;

        let mut workflow = root.obj("workflow")?;
        let workflow_path = workflow.str("path")?.to_string();
        if !workflow_path.starts_with(".github/workflows/") || !is_identifier(&workflow_path) {
            return Err("workflow.path: expected a .github/workflows/ file".to_string());
        }
        let workflow_blob = git_object(&mut workflow, "blob")?;
        let workflow_commit = git_object(&mut workflow, "commit")?;
        let plan_schema = identifier(&mut workflow, "plan_schema")?;
        let plan_digest = digest_field(&mut workflow, "plan_digest")?;
        workflow.finish()?;

        let mut transaction = root.obj("transaction")?;
        let transaction_kind = closed(&mut transaction, "kind", TransactionKind::parse)?;
        let transaction_schema = identifier(&mut transaction, "schema")?;
        let transaction_tool = identifier(&mut transaction, "tool")?;
        let transaction_request_id = identifier(&mut transaction, "request_id")?;
        transaction.finish()?;

        let mut event = root.obj("event")?;
        let event_kind = closed(&mut event, "kind", EventKind::parse)?;
        let event_ref = event.opt_str("ref")?.map(str::to_string);
        event.finish()?;

        let allowed = operations(root.arr("allowed_operations")?, "allowed_operations")?;
        let denied = operations(root.arr("denied_operations")?, "denied_operations")?;
        if let Some(both) = allowed.intersection(&denied).next() {
            return Err(format!(
                "operation `{}` is both allowed and denied",
                both.as_str()
            ));
        }
        if let Some(unstated) = Operation::ALL
            .iter()
            .find(|operation| !allowed.contains(operation) && !denied.contains(operation))
        {
            return Err(format!(
                "operation `{}` is neither allowed nor denied; every operation must be explicit",
                unstated.as_str()
            ));
        }

        let mut channel_obj = root.obj("channels")?;
        let mut channels = BTreeMap::new();
        for channel in Channel::ALL {
            let state = match channel_obj.str(channel.as_str())? {
                "authorized" => true,
                "not_authorized" => false,
                other => {
                    return Err(format!(
                        "channels.{}: `{other}` is not authorized or not_authorized",
                        channel.as_str()
                    ));
                }
            };
            channels.insert(*channel, state);
        }
        channel_obj.finish()?;

        let mut subjects = Vec::new();
        for (index, item) in root.arr("subjects")?.iter().enumerate() {
            let mut subject = Obj::new(item, &format!("$.subjects[{index}]"))?;
            subjects.push(Subject {
                class: closed(&mut subject, "class", SubjectClass::parse)?,
                name: identifier(&mut subject, "name")?,
                sha256: digest_field(&mut subject, "sha256")?,
            });
            subject.finish()?;
        }
        if subjects.windows(2).any(|pair| pair[0].name >= pair[1].name) {
            return Err("subjects: names must be unique and in ascending order".to_string());
        }

        let mut environments = Vec::new();
        for (index, item) in root.arr("environments")?.iter().enumerate() {
            let mut environment = Obj::new(item, &format!("$.environments[{index}]"))?;
            let channel = closed(&mut environment, "channel", Channel::parse)?;
            let name = identifier(&mut environment, "name")?;
            let credentials = ascending_strings(environment.arr("credentials")?, "credentials")?;
            if credentials.iter().any(|name| !is_identifier(name)) {
                return Err("environments: credential entries are identifier names".to_string());
            }
            environments.push(Environment {
                channel,
                name,
                credentials: credentials.into_iter().map(str::to_string).collect(),
            });
            environment.finish()?;
        }
        if environments
            .windows(2)
            .any(|pair| pair[0].channel.as_str() >= pair[1].channel.as_str())
        {
            return Err(
                "environments: one entry per channel, in ascending channel order".to_string(),
            );
        }

        let mut executors = Vec::new();
        for (index, item) in root.arr("executors")?.iter().enumerate() {
            let mut executor = Obj::new(item, &format!("$.executors[{index}]"))?;
            let kind = executor.str("kind")?;
            if !matches!(kind, "app" | "user") {
                return Err(format!("executors[{index}].kind: expected app or user"));
            }
            executors.push((kind.to_string(), executor.id("id")?));
            executor.finish()?;
        }
        if executors.is_empty() || executors.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(
                "executors: at least one entry, unique, ascending by kind then id".to_string(),
            );
        }

        let mut consumption = root.obj("consumption")?;
        if consumption.str("mode")? != "one_use" {
            return Err("consumption.mode: v1 packets are one_use".to_string());
        }
        let idempotency = consumption.str("idempotency")?.to_string();
        if consumption.str("terminal_receipt_schema")? != CONSUMPTION_RECEIPT_SCHEMA {
            return Err(format!(
                "consumption.terminal_receipt_schema: must be {CONSUMPTION_RECEIPT_SCHEMA}"
            ));
        }
        consumption.finish()?;

        fixed_statements(&mut root, "invalidation", INVALIDATIONS)?;
        fixed_statements(&mut root, "stop_conditions", STOP_CONDITIONS)?;
        fixed_statements(&mut root, "non_claims", NON_CLAIMS)?;
        root.finish()?;

        Ok(Self {
            authorization_id,
            repository_id,
            repository_name,
            programme_issue,
            record_issue,
            profile,
            issuer_id,
            issuer_login,
            sequence,
            nonce,
            created_at,
            not_before,
            not_after,
            supersedes,
            version,
            source_commit,
            source_tree,
            evidence,
            policy_ruling,
            workflow_path,
            workflow_blob,
            workflow_commit,
            plan_schema,
            plan_digest,
            transaction_kind,
            transaction_schema,
            transaction_tool,
            transaction_request_id,
            event_kind,
            event_ref,
            allowed,
            channels,
            subjects,
            environments,
            executors,
            idempotency,
        })
    }

    pub(crate) fn authorized_channels(&self) -> Vec<Channel> {
        self.channels
            .iter()
            .filter(|(_, authorized)| **authorized)
            .map(|(channel, _)| *channel)
            .collect()
    }

    /// The closed (profile, transaction) matrix. Anything a boundary does not
    /// name is refused rather than ignored.
    fn check_profile(&self) -> Result<(), String> {
        use Channel as C;
        use EvidenceKind as E;
        use Operation as O;
        use Profile as P;
        use SubjectClass as S;
        use TransactionKind as T;

        struct Boundary {
            operations: &'static [Operation],
            channels: &'static [Channel],
            evidence: &'static [EvidenceKind],
            subjects: &'static [SubjectClass],
            event: EventKind,
        }
        const READ_AND_RECORD: [Operation; 2] = [O::ReadEventEvidence, O::RecordConsumptionReceipt];
        const QUALIFIED: [EvidenceKind; 3] = [E::ShipPacket, E::Readiness, E::Qualification];
        const STABLE: [EvidenceKind; 6] = [
            E::ShipPacket,
            E::Readiness,
            E::Qualification,
            E::PublicRcVerification,
            E::Cohort,
            E::StableDecision,
        ];
        const CONTINUED: [EvidenceKind; 7] = [
            E::ShipPacket,
            E::Readiness,
            E::Qualification,
            E::PublicRcVerification,
            E::Cohort,
            E::StableDecision,
            E::PredecessorReceipt,
        ];
        const GITHUB_SUBJECTS: [SubjectClass; 5] = [
            S::ServerArchive,
            S::ServerManifest,
            S::Checksums,
            S::Vsix,
            S::AttestationBundle,
        ];
        let boundary = match (self.profile, self.transaction_kind) {
            (P::ReservedProbe, T::Probe) => Boundary {
                operations: &[
                    O::CreateAnnotatedTag,
                    O::TriggerNoPublishPlan,
                    O::ReadEventEvidence,
                    O::RecordConsumptionReceipt,
                    O::DeleteExactProbeTag,
                ],
                channels: &[],
                evidence: &[],
                subjects: &[],
                event: EventKind::AnnotatedTagPush,
            },
            (P::PublicRc, T::Github) => Boundary {
                operations: &[
                    O::CreateAnnotatedTag,
                    O::GithubPrereleaseCreate,
                    O::GithubAssetUpload,
                    O::ProvenanceAttest,
                    O::RequestNamedEnvironment,
                    READ_AND_RECORD[0],
                    READ_AND_RECORD[1],
                ],
                channels: &[C::GitTag, C::GithubPrerelease],
                evidence: &QUALIFIED,
                subjects: &GITHUB_SUBJECTS,
                event: EventKind::AnnotatedTagPush,
            },
            (P::StableCore, T::Github) => Boundary {
                operations: &[
                    O::CreateAnnotatedTag,
                    O::GithubReleaseCreate,
                    O::GithubAssetUpload,
                    O::ProvenanceAttest,
                    O::RequestNamedEnvironment,
                    READ_AND_RECORD[0],
                    READ_AND_RECORD[1],
                ],
                channels: &[C::GitTag, C::GithubRelease],
                evidence: &STABLE,
                subjects: &GITHUB_SUBJECTS,
                event: EventKind::AnnotatedTagPush,
            },
            (P::StableCore, T::CratesIo) => Boundary {
                operations: &[
                    O::CratesIoPublish,
                    O::RequestNamedEnvironment,
                    READ_AND_RECORD[0],
                    READ_AND_RECORD[1],
                ],
                channels: &[C::CratesIo],
                evidence: &STABLE,
                subjects: &[S::Crate],
                event: EventKind::ExactChannelContinuation,
            },
            (P::StableChannelContinuation, T::VsMarketplace) => Boundary {
                operations: &[
                    O::VsMarketplacePublish,
                    O::RequestNamedEnvironment,
                    READ_AND_RECORD[0],
                    READ_AND_RECORD[1],
                ],
                channels: &[C::VsMarketplace],
                evidence: &CONTINUED,
                subjects: &[S::Vsix],
                event: EventKind::ExactChannelContinuation,
            },
            (P::StableChannelContinuation, T::OpenVsx) => Boundary {
                operations: &[
                    O::OpenVsxPublish,
                    O::RequestNamedEnvironment,
                    READ_AND_RECORD[0],
                    READ_AND_RECORD[1],
                ],
                channels: &[C::OpenVsx],
                evidence: &CONTINUED,
                subjects: &[S::Vsix],
                event: EventKind::ExactChannelContinuation,
            },
            (P::StableChannelContinuation, T::Npm) => {
                return Err(
                    "npm continuation has no ruled and implemented transaction in v1".to_string(),
                );
            }
            (profile, kind) => {
                return Err(format!(
                    "profile {} cannot carry a {} transaction",
                    profile.as_str(),
                    kind.as_str()
                ));
            }
        };
        let profile = self.profile.as_str();

        if let Some(operation) = self
            .allowed
            .iter()
            .find(|operation| !boundary.operations.contains(operation))
        {
            return Err(format!(
                "profile {profile} cannot allow operation `{}`",
                operation.as_str()
            ));
        }
        if let Some(channel) = self
            .authorized_channels()
            .into_iter()
            .find(|channel| !boundary.channels.contains(channel))
        {
            return Err(format!(
                "profile {profile} cannot authorize channel `{}`",
                channel.as_str()
            ));
        }
        let authorized = self.authorized_channels();
        if self.profile != P::ReservedProbe && authorized.is_empty() {
            return Err(format!("profile {profile} authorizes no channel"));
        }
        if self.profile == P::StableChannelContinuation && authorized.len() != 1 {
            return Err("a continuation authorizes exactly one channel".to_string());
        }

        // A channel and the operation that mutates it stand or fall together.
        let primary = [
            (C::GithubPrerelease, O::GithubPrereleaseCreate),
            (C::GithubRelease, O::GithubReleaseCreate),
            (C::CratesIo, O::CratesIoPublish),
            (C::VsMarketplace, O::VsMarketplacePublish),
            (C::OpenVsx, O::OpenVsxPublish),
            (C::Npm, O::NpmPublish),
        ];
        for (channel, operation) in primary {
            if authorized.contains(&channel) != self.allowed.contains(&operation) {
                return Err(format!(
                    "channel `{}` and operation `{}` must be authorized together",
                    channel.as_str(),
                    operation.as_str()
                ));
            }
        }
        let tag_event = boundary.event == EventKind::AnnotatedTagPush;
        if tag_event != self.allowed.contains(&O::CreateAnnotatedTag) {
            return Err(
                "create_annotated_tag is allowed exactly for tag-backed packets".to_string(),
            );
        }
        if self.profile != P::ReservedProbe && tag_event != authorized.contains(&C::GitTag) {
            return Err(
                "a tag-backed release packet must authorize git_tag, and only it may".to_string(),
            );
        }
        let release_channel =
            authorized.contains(&C::GithubPrerelease) || authorized.contains(&C::GithubRelease);
        for operation in [O::GithubAssetUpload, O::ProvenanceAttest] {
            if self.allowed.contains(&operation) && !release_channel {
                return Err(format!(
                    "operation `{}` requires an authorized GitHub release channel",
                    operation.as_str()
                ));
            }
        }

        for kind in EvidenceKind::ALL {
            let required = boundary.evidence.contains(kind);
            let present = self.evidence.get(kind).is_some_and(Option::is_some);
            if required != present {
                return Err(format!(
                    "evidence.{}: profile {profile} {} it",
                    kind.as_str(),
                    if required { "requires" } else { "forbids" }
                ));
            }
        }
        if (self.profile == P::PublicRc) != self.policy_ruling.is_some() {
            return Err(
                "evidence.policy_ruling: required for public_rc and forbidden elsewhere"
                    .to_string(),
            );
        }

        if boundary.subjects.is_empty() != self.subjects.is_empty() {
            return Err(format!(
                "subjects: profile {profile} {} a subject catalog",
                if self.subjects.is_empty() {
                    "requires"
                } else {
                    "forbids"
                }
            ));
        }
        if let Some(subject) = self
            .subjects
            .iter()
            .find(|subject| !boundary.subjects.contains(&subject.class))
        {
            return Err(format!(
                "subject `{}` has class `{}`, which profile {profile} cannot carry",
                subject.name,
                subject.class.as_str()
            ));
        }
        if self.profile == P::StableChannelContinuation && self.subjects.len() != 1 {
            return Err("a continuation carries exactly one VSIX subject".to_string());
        }

        let mut credentials = BTreeSet::new();
        let mut names = BTreeSet::new();
        for environment in &self.environments {
            if !authorized.contains(&environment.channel) {
                return Err(format!(
                    "environment `{}` is bound to channel `{}`, which is not authorized",
                    environment.name,
                    environment.channel.as_str()
                ));
            }
            if !names.insert(environment.name.as_str()) {
                return Err(format!(
                    "environment `{}` is shared between channels",
                    environment.name
                ));
            }
            for credential in &environment.credentials {
                if !credentials.insert(credential.as_str()) {
                    return Err(format!(
                        "credential name `{credential}` is shared between environments"
                    ));
                }
            }
        }
        if self.environments.is_empty() == self.allowed.contains(&O::RequestNamedEnvironment) {
            return Err(
                "request_named_environment is allowed exactly when an environment is named"
                    .to_string(),
            );
        }
        if self.profile == P::StableChannelContinuation && self.environments.len() != 1 {
            return Err("a continuation names exactly one environment".to_string());
        }

        if self.event_kind != boundary.event {
            return Err(format!(
                "event.kind: profile {profile} with a {} transaction requires `{}`",
                self.transaction_kind.as_str(),
                boundary.event.as_str()
            ));
        }
        let version_shape = VersionShape::of(&self.version);
        let expected_shape = match self.profile {
            P::ReservedProbe => VersionShape::Probe,
            P::PublicRc => VersionShape::ReleaseCandidate,
            P::StableCore | P::StableChannelContinuation => VersionShape::Stable,
        };
        if version_shape != Some(expected_shape) {
            return Err(format!(
                "version `{}` is not a {profile} version",
                self.version
            ));
        }
        if self.profile == P::ReservedProbe
            && self.version.strip_prefix(PROBE_VERSION_PREFIX) != Some(self.nonce.as_str())
        {
            return Err("a probe version must end in this packet's nonce".to_string());
        }
        let expected_ref = tag_event.then(|| format!("refs/tags/v{}", self.version));
        if self.event_ref != expected_ref {
            return Err(match expected_ref {
                Some(expected) => format!("event.ref: must be exactly `{expected}`"),
                None => "event.ref: a continuation carries no ref".to_string(),
            });
        }

        let expected_idempotency = if self.profile == P::ReservedProbe {
            "none"
        } else {
            "verify_only_when_exact"
        };
        if self.idempotency != expected_idempotency {
            return Err(format!(
                "consumption.idempotency: profile {profile} requires `{expected_idempotency}`"
            ));
        }

        if !(self.created_at <= self.not_before && self.not_before < self.not_after) {
            return Err("validity: requires created_at <= not_before < not_after".to_string());
        }
        let limit = if self.profile == P::ReservedProbe {
            PROBE_MAX_WINDOW_SECONDS
        } else {
            MAX_WINDOW_SECONDS
        };
        if self.not_after - self.not_before > limit {
            return Err(format!(
                "validity: profile {profile} allows a window of at most {} hours",
                limit / 3_600
            ));
        }
        Ok(())
    }
}

/// A packet names environments and credentials; it never carries a value.
fn no_secret_shaped_text(value: &Value) -> Result<(), String> {
    match value {
        Value::String(text) => match super::secret_shaped(text) {
            Some(kind) => Err(format!(
                "the packet contains a credential-shaped value ({kind}); value withheld"
            )),
            None => Ok(()),
        },
        Value::Array(items) => items.iter().try_for_each(no_secret_shaped_text),
        Value::Object(map) => map.values().try_for_each(no_secret_shaped_text),
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn operations(items: &[Value], path: &str) -> Result<BTreeSet<Operation>, String> {
    ascending_strings(items, path)?
        .into_iter()
        .map(|text| {
            Operation::parse(text).ok_or_else(|| format!("{path}: unknown operation `{text}`"))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VersionShape {
    Stable,
    ReleaseCandidate,
    Probe,
}

impl VersionShape {
    fn of(version: &str) -> Option<Self> {
        if let Some(nonce) = version.strip_prefix(PROBE_VERSION_PREFIX) {
            return (nonce.len() == 32 && is_lower_hex(nonce)).then_some(Self::Probe);
        }
        let (core, candidate) = match version.split_once("-rc.") {
            Some((core, number)) => (core, Some(number)),
            None => (version, None),
        };
        let numeric = |text: &str| parse_plain_u64(text).is_some();
        let parts = core.split('.').collect::<Vec<_>>();
        if parts.len() != 3 || !parts.iter().all(|part| numeric(part)) || core == "0.0.0" {
            return None;
        }
        match candidate {
            None => Some(Self::Stable),
            Some(number) if numeric(number) && number != "0" => Some(Self::ReleaseCandidate),
            Some(_) => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
