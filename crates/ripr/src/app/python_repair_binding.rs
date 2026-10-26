//! Digest-bound bridge between the two-phase external-edit driver (#2443),
//! the durable repair-attempt authority (#2927), and the governed Python
//! repair-trust selection/attempt model (RIPR-SPEC-0176, #3568).
//!
//! The bridge is keyed by digests, never by names: the driver consumes the
//! accepted selection manifest (#3568) by its exact-bytes sha256, binds one
//! selection row by its recomputed canonical `selection_digest`, and retains
//! that binding as a staged artifact of the durable `RepairAttemptId`
//! transaction. The after phase re-verifies the same digests immediately
//! before recording an applied edit, so a replaced row or a changed manifest
//! fails closed before the attempt advances.
//!
//! Claim boundary: the driver records preparation and application evidence
//! only. It records no verification result, no static movement, and no
//! closure; RIPR-SPEC-0176's `verified`/`reviewed`/`accepted` lifecycle
//! states and the movement/execution axes stay owned by the later phase
//! (#3570) and the corpus, never by this driver.
//!
//! Test-only edit-surface guarantee: this module's guarantee is layered —
//! the repair packet producer's test-selection authority (the packet names
//! the focused test as the selected edit target, and the cage is derived
//! from it), the positive test-surface gate enforced where the cage policy
//! is constructed (`edit_cage_policy_from_packet` refuses any selected
//! target that is not a recognized test surface — a `tests`/`test` path
//! component or a bounded `test_*`/`*_test`/`*_tests` file-name convention —
//! before any cage exists, so a production path-shaped value can never
//! become the authored edit target), plus the production/generated/vendor/
//! environment denial families (`is_denied_edit_surface`) kept as defense in
//! depth on every row bind. A parser-backed positive allowlist of test paths
//! remains the later refinement; it folds into #3727's parser-backed facts
//! if it lands there.

use crate::edit_cage::EditCagePolicy;
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Wire schema of the driver binding record. Versioned independently of the
/// durable attempt manifest schema.
pub(crate) const BINDING_SCHEMA_VERSION: &str = "0.1";
pub(crate) const BINDING_KIND: &str = "python_repair_driver_binding";
/// The binding consumes and extends the #3568 model, so it carries that spec
/// identity; it introduces no new spec number.
pub(crate) const BINDING_SPEC: &str = "RIPR-SPEC-0176";
/// Artifact role under which the prepare-phase record is staged into the
/// durable attempt.
pub(crate) const BINDING_ARTIFACT_ROLE: &str = "python_repair_trust_binding";
/// Repository-global compatibility projection of the prepare record.
pub(crate) const PREPARE_RECORD_COMPAT_PATH: &str =
    "target/ripr/workflow/python-repair-trust-binding.json";
/// Repository-global compatibility projection of the apply record.
pub(crate) const APPLY_RECORD_COMPAT_PATH: &str =
    "target/ripr/workflow/python-repair-driver-after.json";

const MANIFEST_SCHEMA_VERSION: &str = "0.1";
const MANIFEST_KIND: &str = "python_repair_trust_manifest";
const SELECTION_DIGEST_FIELD: &str = "selection_digest";

/// The binding is issued only for a target the driver can cage exactly: an
/// existing, unambiguous, test-only file. `proposed`/`ambiguous`/
/// `unavailable`/`unsafe` targets each require a new or re-authorized
/// selection before the driver prepares an edit transaction.
const BINDABLE_TARGET_STATE: &str = "existing";

/// Declared telemetry of the binding record: the only field that may differ
/// between two equivalent preparations. It records where the selection
/// manifest was read from so the after phase can re-read the same bytes; it
/// is not an identity (the digest is).
pub(crate) const TELEMETRY_MANIFEST_PATH_FIELD: &str = "selection_manifest_path";

/// The explicit authorization signals. Both are required together at prepare
/// and at apply; the driver never infers, defaults, or persists an
/// authorization that was not handed to it on the exact invocation.
pub(crate) const AUTHORIZATION_STATUS: &str = "granted";
pub(crate) const AUTHORIZATION_METHOD: &str = "explicit-operator-flags";

/// The driver's standing non-claims. The apply record carries the edit-cage
/// decision (a structural verdict on which paths changed), never a
/// verification verdict, never movement, never closure.
pub(crate) const BINDING_NON_CLAIMS: [&str; 3] = [
    "no verification result is claimed by the driver",
    "no static movement is claimed by the driver",
    "no closure is claimed by the driver",
];

/// The closed prepare-record field set (deny-unknown). Retained binding
/// records are always prepare records; the apply-phase additions live only in
/// the freshly rendered apply record.
const PREPARE_RECORD_KEYS: [&str; 14] = [
    "schema_version",
    "kind",
    "spec",
    "phase",
    "seam_id",
    "repository_head",
    TELEMETRY_MANIFEST_PATH_FIELD,
    "driver",
    "config",
    "input",
    "trust",
    "edit_surface",
    "authorization",
    "non_claims",
];

const TRUST_KEYS: [&str; 18] = [
    "attempt_id",
    "case_id",
    "subject_id",
    "repository",
    "base",
    "head",
    "tree",
    "source_currentness",
    "selection_manifest_sha256",
    "selection_digest",
    "target_path",
    "target_state",
    "family",
    "owner",
    "discriminator",
    "relation",
    "oracle",
    "limitation",
];

const DRIVER_KEYS: [&str; 2] = ["binary_sha256", "version"];
const CONFIG_KEYS: [&str; 1] = ["profile"];
const INPUT_KEYS: [&str; 2] = ["packet_sha256", "before_snapshot_sha256"];
const EDIT_SURFACE_KEYS: [&str; 2] = ["allowed", "forbidden"];
const AUTHORIZATION_KEYS: [&str; 3] = ["status", "authority", "method"];

/// Denied edit-surface path prefixes, mirroring the RIPR-SPEC-0176 denied
/// production/generated/vendor/environment vocabulary. The invariant is
/// shared across the crate boundary by digest-bound records, while each
/// matcher stays local by design (the xtask validator owns corpus
/// enforcement; this crate owns the fail-closed gate at prepare time).
const DENIED_SURFACE_PREFIXES: [&str; 14] = [
    "target/",
    "dist/",
    "build/",
    "vendor/",
    "vendored/",
    "node_modules/",
    "generated/",
    "__pycache__/",
    "site-packages/",
    ".venv/",
    "venv/",
    "env/",
    ".tox/",
    ".eggs/",
];

fn is_denied_edit_surface(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    DENIED_SURFACE_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
        || lowered
            .split('/')
            .any(|component| component.contains(".generated."))
}

/// CLI-provided edit authorization. `authorized` is true only when the
/// explicit flag was passed; `authority` is the non-empty operator/agent
/// identity supplied with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditAuthorization {
    pub(crate) authorized: bool,
    pub(crate) authority: Option<String>,
}

impl EditAuthorization {
    /// The typed refusal follows the bounded-execution authorization
    /// precedent: it names the missing signals instead of failing silently,
    /// and the driver never infers an authorization.
    fn verify(&self, action: &str) -> Result<&str, String> {
        if !self.authorized {
            return Err(format!(
                "python repair-trust binding refuses to {action}: explicit --edit-authorized and --edit-authority <identity> are required; the driver never authorizes an edit automatically"
            ));
        }
        let authority = self
            .authority
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "python repair-trust binding refuses to {action}: --edit-authorized requires --edit-authority <identity>"
                )
            })?;
        Ok(authority)
    }
}

/// CLI-provided reference into the accepted #3568 selection manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PythonRepairTrustSelection {
    pub(crate) manifest_path: PathBuf,
    pub(crate) attempt_id: String,
}

/// One verified selection row, reduced to the identities the binding record
/// retains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedSelection {
    pub(crate) attempt_id: String,
    pub(crate) selection_manifest_sha256: String,
    pub(crate) selection_manifest_path: String,
    pub(crate) selection_digest: String,
    pub(crate) case_id: String,
    pub(crate) subject_id: String,
    pub(crate) repository: String,
    pub(crate) base: String,
    pub(crate) head: String,
    pub(crate) tree: Option<String>,
    pub(crate) source_currentness: Option<String>,
    pub(crate) family: String,
    pub(crate) owner: String,
    pub(crate) discriminator: String,
    pub(crate) relation: String,
    pub(crate) oracle: String,
    pub(crate) limitation: Option<String>,
    pub(crate) target_path: String,
    pub(crate) target_state: String,
}

/// The staged prepare-phase binding artifact: the digest the durable attempt
/// manifest records for it (prefix-stripped to the corpus digest shape) and
/// its parsed record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedBinding {
    pub(crate) artifact_sha256: String,
    pub(crate) value: Value,
}

/// Outcome of a successful prepare-phase binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedBinding {
    pub(crate) record: Value,
    pub(crate) record_path: PathBuf,
    pub(crate) verified: VerifiedSelection,
}

// ---------------------------------------------------------------------------
// Strict JSON (duplicate keys fail at load) and digest helpers
// ---------------------------------------------------------------------------

struct StrictJson(Value);

impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

struct StrictJsonVisitor;

impl<'de> Visitor<'de> for StrictJsonVisitor {
    type Value = StrictJson;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("JSON without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJson(value.into()))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJson(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJson(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .map(StrictJson)
            .ok_or_else(|| serde::de::Error::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJson(value.into()))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictJson(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(StrictJson(value)) = sequence.next_element::<StrictJson>()? {
            values.push(value);
        }
        Ok(StrictJson(Value::Array(values)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let StrictJson(value) = map.next_value::<StrictJson>()?;
            if values.insert(key.clone(), value).is_some() {
                return Err(serde::de::Error::custom(format!(
                    "duplicate object key `{key}`"
                )));
            }
        }
        Ok(StrictJson(Value::Object(values)))
    }
}

/// The one strict-JSON entry point for binding and manifest loads: duplicate
/// keys fail at load (structural rot), matching the corpus validator canon.
fn parse_strict_json(text: &str) -> Result<Value, String> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let StrictJson(value) =
        StrictJson::deserialize(&mut deserializer).map_err(|error| error.to_string())?;
    deserializer.end().map_err(|error| error.to_string())?;
    Ok(value)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut rendered = String::with_capacity(64);
    for byte in digest {
        rendered.push_str(&format!("{byte:02x}"));
    }
    rendered
}

/// Digest text is bare lowercase sha256 hex (64 characters), the #3568 digest
/// shape (the durable attempt artifacts keep their `sha256:`-prefixed shape;
/// the binding record carries the corpus shape so the identities transfer
/// verbatim).
fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn is_git_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// A portable repo-relative path: forward slashes, never absolute, no `..`
/// components. Mirrors the RIPR-SPEC-0176 portability contract.
fn check_portable_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("path must be non-empty".to_string());
    }
    if path.contains('\\') {
        return Err(format!(
            "path `{path}` is not portable: backslash separators are not allowed"
        ));
    }
    if path.starts_with('/') {
        return Err(format!("path `{path}` must be repo-relative, not absolute"));
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Err(format!(
            "path `{path}` must be repo-relative, not a drive-letter absolute path"
        ));
    }
    if path.split('/').any(|component| component == "..") {
        return Err(format!("path `{path}` must not contain `..` components"));
    }
    Ok(())
}

/// Normalizes a portable repo-relative path: `./` segments dropped, empty
/// segments collapsed. The normalized form is what is bound: it is compared
/// against the packet's selected edit target, matched in the repository
/// inventory, and recorded, so the raw spellings `./tests/x.rs` and
/// `tests//x.rs` bind identically to `tests/x.rs`. Callers run the
/// portability check on the raw spelling first, so absolute and
/// `..`-carrying paths never reach this function.
fn normalize_repo_relative_path(path: &str) -> String {
    path.split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------------------
// Small strict-object accessors
// ---------------------------------------------------------------------------

fn as_object<'a>(value: &'a Value, subject: &str) -> Result<&'a Map<String, Value>, String> {
    value.as_object().ok_or_else(|| {
        format!("python repair-trust binding: subject=`{subject}`: must be a JSON object")
    })
}

fn reject_unknown_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    subject: &str,
) -> Result<(), String> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "python repair-trust binding: subject=`{subject}`: unknown field `{key}` (denied to catch schema rot and typos)"
            ));
        }
    }
    Ok(())
}

fn require_string(
    subject: &str,
    object: &Map<String, Value>,
    field: &str,
) -> Result<String, String> {
    match object.get(field) {
        None => Err(format!(
            "python repair-trust binding: subject=`{subject}` field=`{field}`: required field is missing"
        )),
        Some(Value::Null) => Err(format!(
            "python repair-trust binding: subject=`{subject}` field=`{field}`: a present null is not a value"
        )),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(text.clone()),
        Some(Value::String(_)) => Err(format!(
            "python repair-trust binding: subject=`{subject}` field=`{field}`: required field must be non-empty"
        )),
        Some(_) => Err(format!(
            "python repair-trust binding: subject=`{subject}` field=`{field}`: field must be a string"
        )),
    }
}

fn opt_string(object: &Map<String, Value>, field: &str) -> Result<Option<String>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.clone())),
        Some(Value::String(_)) => Err(format!(
            "python repair-trust binding: field=`{field}`: string field must be non-empty when present"
        )),
        Some(_) => Err(format!(
            "python repair-trust binding: field=`{field}`: field must be a string when present"
        )),
    }
}

/// Canonical selection-row digest: sha256 over the row JSON with only
/// `selection_digest` removed, re-serialized as compact UTF-8 JSON. The
/// preimage is collected into a `BTreeMap` first, so the key order is sorted
/// regardless of any workspace-level serde_json `preserve_order` feature
/// (with it, `Map` iterates in insertion order and the bytes would vary by
/// construction order; rows are flat string-valued objects, so top-level
/// sorting is the whole canonicalization). This is the same preimage the
/// corpus validator recomputes.
fn canonical_selection_digest(row: &Map<String, Value>) -> Result<String, String> {
    let mut canonical = row.clone();
    canonical.remove(SELECTION_DIGEST_FIELD);
    let sorted: std::collections::BTreeMap<String, Value> = canonical.into_iter().collect();
    let text = serde_json::to_string(&sorted)
        .map_err(|error| format!("canonical selection serialization failed: {error}"))?;
    Ok(sha256_hex(text.as_bytes()))
}

// ---------------------------------------------------------------------------
// Selection-manifest verification
// ---------------------------------------------------------------------------

/// Loads and structurally validates the accepted #3568 selection manifest far
/// enough to bind one row by digest: envelope identity, closed row schema,
/// unique attempt identities, and the row digest anchors. Full corpus
/// semantics remain owned by the xtask validator; this is the fail-closed
/// consumer-side verification.
fn load_selection_manifest(
    manifest_path: &Path,
) -> Result<(String, Vec<Map<String, Value>>), String> {
    let bytes = std::fs::read(manifest_path).map_err(|error| {
        format!(
            "python repair-trust selection manifest {} is not readable: {error}",
            manifest_path.display()
        )
    })?;
    let text = String::from_utf8(bytes)
        .map_err(|error| format!("python repair-trust selection manifest is not UTF-8: {error}"))?;
    let value = parse_strict_json(&text).map_err(|error| {
        format!("python repair-trust selection manifest is not well-formed JSON: {error}")
    })?;
    let top = as_object(&value, "selection manifest")?;
    reject_unknown_keys(
        top,
        &[
            "schema_version",
            "kind",
            "spec",
            "description",
            "selections",
        ],
        "selection manifest",
    )?;
    for (field, expected) in [
        ("schema_version", MANIFEST_SCHEMA_VERSION),
        ("kind", MANIFEST_KIND),
        ("spec", BINDING_SPEC),
    ] {
        let actual = require_string("selection manifest", top, field)?;
        if actual != expected {
            return Err(format!(
                "python repair-trust selection manifest field `{field}` must be `{expected}`, got `{actual}`"
            ));
        }
    }
    let rows = match value.get("selections") {
        Some(Value::Array(rows)) => rows,
        _ => {
            return Err(
                "python repair-trust selection manifest must contain a selections array"
                    .to_string(),
            );
        }
    };
    let mut parsed = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rows {
        let entry = as_object(row, "selection row")?;
        reject_unknown_keys(
            entry,
            &[
                "attempt_id",
                "case_id",
                "subject_id",
                "repository",
                "base",
                "head",
                "tree",
                "source_currentness",
                "selection_reason",
                "diversity_stratum",
                "family",
                "owner",
                "discriminator",
                "relation",
                "oracle",
                "limitation",
                "expected_direction",
                "claim_boundary",
                "target_path",
                "target_state",
                "selected_at",
                "selector",
                "authority_snapshot_digest",
                "selection_digest",
            ],
            "selection row",
        )?;
        let attempt_id = require_string("selection row", entry, "attempt_id")?;
        if !seen.insert(attempt_id.clone()) {
            return Err(format!(
                "python repair-trust selection manifest carries duplicate attempt identity `{attempt_id}`"
            ));
        }
        parsed.push(entry.clone());
    }
    Ok((sha256_hex(text.as_bytes()), parsed))
}

/// Verifies one selection row's digest anchors, shape, and target-state and
/// surface policy. Shared by the prepare and apply phases; the phase-specific
/// checks (packet alignment, ground truth, head pin) live with their callers.
fn verify_row_binding(
    row: &Map<String, Value>,
    request_attempt_id: &str,
) -> Result<VerifiedSelection, String> {
    let attempt_id = require_string("selection row", row, "attempt_id")?;
    if attempt_id != request_attempt_id {
        return Err(format!(
            "selection row identity `{attempt_id}` does not match the requested attempt `{request_attempt_id}`"
        ));
    }
    let recorded = require_string(&attempt_id, row, SELECTION_DIGEST_FIELD)?;
    if !is_sha256_hex(&recorded) {
        return Err(format!(
            "selection row `{attempt_id}` selection_digest must be bare lowercase sha256 hex"
        ));
    }
    let recomputed = canonical_selection_digest(row)
        .map_err(|error| format!("selection row `{attempt_id}`: {error}"))?;
    if recorded != recomputed {
        return Err(format!(
            "selection row `{attempt_id}` digest mismatch: recorded `{recorded}` but the canonical content digests to `{recomputed}`; a replaced or edited selected row requires a new selection"
        ));
    }

    let raw_target_path = require_string(&attempt_id, row, "target_path")?;
    check_portable_path(&raw_target_path)
        .map_err(|error| format!("selection row `{attempt_id}` target_path: {error}"))?;
    // The normalized spelling is what is bound: it is compared against the
    // packet's selected target, matched in the repository inventory, and
    // recorded.
    let target_path = normalize_repo_relative_path(&raw_target_path);
    let target_state = require_string(&attempt_id, row, "target_state")?;
    if target_state != BINDABLE_TARGET_STATE {
        return Err(format!(
            "selection row `{attempt_id}` declares target_state `{target_state}`; the driver binds only `{BINDABLE_TARGET_STATE}` targets because a changed or ambiguous target requires a new or re-authorized selection"
        ));
    }
    if is_denied_edit_surface(&target_path) {
        return Err(format!(
            "selection row `{attempt_id}` target `{target_path}` falls under a production/generated/vendor/environment edit surface; test-only paths are the only allowed edit surface and this selection must not be bound"
        ));
    }

    let row_head = require_string(&attempt_id, row, "head")?;
    if !is_git_sha(&row_head) {
        return Err(format!(
            "selection row `{attempt_id}` head must be a bare lowercase 40-character commit SHA"
        ));
    }
    let base = require_string(&attempt_id, row, "base")?;
    if !is_git_sha(&base) {
        return Err(format!(
            "selection row `{attempt_id}` base must be a bare lowercase 40-character commit SHA"
        ));
    }

    let tree = opt_string(row, "tree")?;
    if let Some(tree) = &tree
        && !is_sha256_hex(tree)
    {
        return Err(format!(
            "selection row `{attempt_id}` tree must be bare lowercase sha256 hex when present"
        ));
    }
    let source_currentness = opt_string(row, "source_currentness")?;
    let limitation = opt_string(row, "limitation")?;
    let case_id = require_string(&attempt_id, row, "case_id")?;
    let subject_id = require_string(&attempt_id, row, "subject_id")?;
    let repository = require_string(&attempt_id, row, "repository")?;
    let family = require_string(&attempt_id, row, "family")?;
    let owner = require_string(&attempt_id, row, "owner")?;
    let discriminator = require_string(&attempt_id, row, "discriminator")?;
    let relation = require_string(&attempt_id, row, "relation")?;
    let oracle = require_string(&attempt_id, row, "oracle")?;

    Ok(VerifiedSelection {
        attempt_id,
        selection_manifest_sha256: String::new(),
        selection_manifest_path: String::new(),
        selection_digest: recorded,
        case_id,
        subject_id,
        repository,
        base,
        head: row_head,
        tree,
        source_currentness,
        family,
        owner,
        discriminator,
        relation,
        oracle,
        limitation,
        target_path,
        target_state,
    })
}

/// The prepare-phase row checks beyond the shared binding checks: the target
/// must resolve to exactly one file in the repository inventory (ground truth
/// before identity), the resolved target must agree with the packet's
/// selected edit target, and the row's `head` pin must equal the repository's
/// current HEAD (a selection made at another commit is stale before the
/// attempt exists).
fn verify_row_for_prepare(
    row: &Map<String, Value>,
    request_attempt_id: &str,
    repository_head: &str,
    policy: &EditCagePolicy,
    root: &Path,
) -> Result<VerifiedSelection, String> {
    let verified = verify_row_binding(row, request_attempt_id)?;
    if verified.head != repository_head {
        return Err(format!(
            "stale selection: row `{}` pins head `{}` but the repository HEAD is `{repository_head}`; a changed repository state requires a new selection before editing",
            verified.attempt_id, verified.head
        ));
    }
    // Ground truth first: the target must resolve to exactly one file in the
    // repository inventory. Zero matches mean the target does not exist; more
    // than one case-insensitive match means the target identity is ambiguous
    // on this checkout. Both fail before editing.
    let matches = repository_target_matches(root, &verified.target_path)?;
    if matches.is_empty() {
        return Err(format!(
            "selection row `{}` target `{}` matches no file in the repository inventory; a zero-match target requires a new selection before editing",
            verified.attempt_id, verified.target_path
        ));
    }
    if matches.len() > 1 {
        return Err(format!(
            "selection row `{}` target `{}` matches {} candidate files in the repository inventory; a multiple-match target is ambiguous and requires a new selection before editing",
            verified.attempt_id,
            verified.target_path,
            matches.len()
        ));
    }
    // Then identity: the row's target must be the packet's selected edit
    // target, never a merely similar name.
    if policy.selected_target.path() != verified.target_path {
        return Err(format!(
            "wrong target: selection row `{}` names `{}` but the repair packet's selected edit target is `{}`; the driver binds one attempt only when both identities agree",
            verified.attempt_id,
            verified.target_path,
            policy.selected_target.path()
        ));
    }
    Ok(verified)
}

/// The apply-phase alignment check: the row's target identity must still
/// agree with the packet's selected edit target.
fn verify_row_target_alignment(
    verified: &VerifiedSelection,
    policy: &EditCagePolicy,
) -> Result<(), String> {
    if policy.selected_target.path() != verified.target_path {
        return Err(format!(
            "wrong target: selection row `{}` names `{}` but the repair packet's selected edit target is `{}`; the driver records one applied edit only when both identities agree",
            verified.attempt_id,
            verified.target_path,
            policy.selected_target.path()
        ));
    }
    Ok(())
}

/// The tracked + untracked repository paths whose case-folded spelling equals
/// the target's. Bounded through the shared git deadline/limit authority.
fn repository_target_matches(root: &Path, target_path: &str) -> Result<Vec<String>, String> {
    let mut inventory = git_inventory(root, &["ls-files", "-z"])?;
    inventory.extend(git_inventory(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?);
    let lowered_target = target_path.to_ascii_lowercase();
    let mut matches = Vec::new();
    for path in inventory {
        if path.to_ascii_lowercase() == lowered_target {
            matches.push(path);
        }
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

fn git_inventory(root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let output = crate::git::run_git_output_with_deadline_and_limit(
        root,
        args,
        Duration::from_secs(30),
        4 * 1024 * 1024,
    )
    .map_err(|error| format!("python repair-trust binding repository inventory failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "python repair-trust binding repository inventory failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

// ---------------------------------------------------------------------------
// Binding records
// ---------------------------------------------------------------------------

/// Identity inputs of one binding record, fixed at render time so the record
/// stays deterministic apart from the declared telemetry path.
struct RecordIdentity<'a> {
    seam_id: &'a str,
    repository_head: &'a str,
    phase: &'a str,
    durable_attempt_id: Option<&'a str>,
    binding_artifact_sha256: Option<&'a str>,
    verified: &'a VerifiedSelection,
    authority: &'a str,
    packet_sha256: &'a str,
    before_snapshot_sha256: &'a str,
    policy: &'a EditCagePolicy,
}

fn render_record(
    root: &Path,
    identity: RecordIdentity<'_>,
    apply: Option<Value>,
) -> Result<Value, String> {
    let verified = identity.verified;
    let driver_exe = std::env::current_exe().map_err(|error| {
        format!("python repair-trust binding could not identify the running driver binary: {error}")
    })?;
    let driver_bytes = std::fs::read(&driver_exe).map_err(|error| {
        format!(
            "python repair-trust binding could not read the running driver binary {}: {error}",
            driver_exe.display()
        )
    })?;
    let allowed = identity
        .policy
        .allowed_edit_surface
        .iter()
        .map(|rule| rule.path().to_string())
        .collect::<Vec<_>>();
    let forbidden = identity
        .policy
        .forbidden_paths
        .iter()
        .map(|rule| rule.path().to_string())
        .collect::<Vec<_>>();
    let mut trust = serde_json::json!({
        "attempt_id": verified.attempt_id,
        "case_id": verified.case_id,
        "subject_id": verified.subject_id,
        "repository": verified.repository,
        "base": verified.base,
        "head": verified.head,
        "selection_manifest_sha256": verified.selection_manifest_sha256,
        "selection_digest": verified.selection_digest,
        "target_path": verified.target_path,
        "target_state": verified.target_state,
        "family": verified.family,
        "owner": verified.owner,
        "discriminator": verified.discriminator,
        "relation": verified.relation,
        "oracle": verified.oracle,
    });
    let trust_object = trust
        .as_object_mut()
        .ok_or_else(|| "binding record construction failed: trust must be an object".to_string())?;
    if let Some(tree) = &verified.tree {
        trust_object.insert("tree".to_string(), Value::String(tree.clone()));
    }
    if let Some(currentness) = &verified.source_currentness {
        trust_object.insert(
            "source_currentness".to_string(),
            Value::String(currentness.clone()),
        );
    }
    if let Some(limitation) = &verified.limitation {
        trust_object.insert("limitation".to_string(), Value::String(limitation.clone()));
    }
    let mut record = serde_json::json!({
        "schema_version": BINDING_SCHEMA_VERSION,
        "kind": BINDING_KIND,
        "spec": BINDING_SPEC,
        "phase": identity.phase,
        "seam_id": identity.seam_id,
        "repository_head": identity.repository_head,
        TELEMETRY_MANIFEST_PATH_FIELD: verified.selection_manifest_path,
        "driver": {
            "binary_sha256": sha256_hex(&driver_bytes),
            "version": env!("CARGO_PKG_VERSION"),
        },
        "config": {
            "profile": detect_config_profile(root),
        },
        "input": {
            "packet_sha256": identity.packet_sha256,
            "before_snapshot_sha256": identity.before_snapshot_sha256,
        },
        "trust": trust,
        "edit_surface": {
            "allowed": allowed,
            "forbidden": forbidden,
        },
        "authorization": {
            "status": AUTHORIZATION_STATUS,
            "authority": identity.authority,
            "method": AUTHORIZATION_METHOD,
        },
        "non_claims": BINDING_NON_CLAIMS,
    });
    let object = record.as_object_mut().ok_or_else(|| {
        "binding record construction failed: record must be an object".to_string()
    })?;
    if let Some(attempt_id) = identity.durable_attempt_id {
        object.insert(
            "durable_attempt_id".to_string(),
            Value::String(attempt_id.to_string()),
        );
    }
    if let Some(artifact_digest) = identity.binding_artifact_sha256 {
        object.insert(
            "binding_artifact_sha256".to_string(),
            Value::String(artifact_digest.to_string()),
        );
    }
    if let Some(apply) = apply {
        object.insert("apply".to_string(), apply);
    }
    Ok(record)
}

/// The analyzed root's config identity, detected from the real producer (the
/// analyzer loads `ripr.toml` from the analyzed root when present), following
/// the eval-sweep config-profile vocabulary. Presence uses the same rule as
/// config discovery: a dangling `ripr.toml` link is present, not defaults.
fn detect_config_profile(root: &Path) -> String {
    if crate::config::config_present_at_root(root) {
        "subject-ripr-toml".to_string()
    } else {
        "default".to_string()
    }
}

fn write_record_file(path: &Path, record: &Value) -> Result<(), String> {
    let mut rendered = serde_json::to_vec_pretty(record)
        .map_err(|error| format!("serialize python repair-trust binding record failed: {error}"))?;
    rendered.push(b'\n');
    // Compatibility projections are shared repository-global files, so they
    // publish through the staged atomic-rename pattern (the same semantics as
    // the eval-sweep report's accepted writes): a concurrent reader never
    // observes partial bytes.
    crate::app::repair_attempt::replace_file_atomically(path, &rendered)
}

// ---------------------------------------------------------------------------
// Prepare phase
// ---------------------------------------------------------------------------

/// Verifies the accepted selection manifest and one requested row, then
/// renders the prepare-phase binding record. Every failure here happens
/// before the durable attempt is published, so no edit transaction can begin
/// from a stale, ambiguous, unsafe, or unauthorized selection.
pub(crate) fn prepare_binding(
    root: &Path,
    seam_id: &str,
    policy: &EditCagePolicy,
    before_snapshot_path: &Path,
    packet_bytes: &[u8],
    request: &PythonRepairTrustSelection,
    authorization: &EditAuthorization,
) -> Result<PreparedBinding, String> {
    let authority = authorization.verify("prepare the edit transaction")?;
    // Resolve the cited manifest relative to the selected workspace root (the
    // same semantics as every other workflow artifact path), never relative to
    // the process working directory: `ripr agent repair --root <ws>
    // --python-repair-trust-manifest target/ripr/trust-manifest.json` must
    // read `<ws>/target/ripr/trust-manifest.json` regardless of where the
    // driver was launched. The resolved location is then canonicalized before
    // the root containment check, so the manifest is judged by where it
    // actually lives, not by how it was spelled.
    let cited_manifest = if request.manifest_path.is_absolute() {
        request.manifest_path.clone()
    } else {
        root.join(&request.manifest_path)
    };
    let manifest_location = std::fs::canonicalize(&cited_manifest).map_err(|error| {
        format!(
            "python repair-trust selection manifest {} is not readable: {error}",
            request.manifest_path.display()
        )
    })?;
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|error| format!("canonicalize repository root failed: {error}"))?;
    if !manifest_location.starts_with(&canonical_root) {
        return Err(format!(
            "python repair-trust selection manifest {} is outside the repository root; a binding must cite a manifest inside the repository it governs",
            request.manifest_path.display()
        ));
    }
    let (manifest_sha256, rows) = load_selection_manifest(&manifest_location)
        .map_err(|error| format!("stale packet rejected before editing: {error}"))?;
    let row = rows
        .iter()
        .find(|row| {
            row.get("attempt_id")
                .and_then(Value::as_str)
                .map(|value| value == request.attempt_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            format!(
                "selection attempt `{}` names an attempt identity outside the accepted selection denominator ({} rows present); selected rows cannot be substituted by name",
                request.attempt_id,
                rows.len()
            )
        })?;
    let repository_head = crate::agent::artifact::current_git_head(root).map_err(|error| {
        format!("python repair-trust binding requires a concrete repository HEAD: {error}")
    })?;
    let mut verified =
        verify_row_for_prepare(row, &request.attempt_id, &repository_head, policy, root)?;
    verified.selection_manifest_sha256 = manifest_sha256;
    // The telemetry path records the resolved absolute location so the after
    // phase re-reads the same bytes regardless of its own working directory.
    verified.selection_manifest_path = manifest_location.display().to_string();

    let packet_sha256 = sha256_hex(packet_bytes);
    let before_bytes = std::fs::read(before_snapshot_path).map_err(|error| {
        format!(
            "python repair-trust binding requires the before snapshot {}: {error}",
            before_snapshot_path.display()
        )
    })?;
    let before_snapshot_sha256 = sha256_hex(&before_bytes);
    let record = render_record(
        root,
        RecordIdentity {
            seam_id,
            repository_head: &repository_head,
            phase: "prepare",
            durable_attempt_id: None,
            binding_artifact_sha256: None,
            verified: &verified,
            authority,
            packet_sha256: &packet_sha256,
            before_snapshot_sha256: &before_snapshot_sha256,
            policy,
        },
        None,
    )?;
    let record_path = root.join(PREPARE_RECORD_COMPAT_PATH);
    write_record_file(&record_path, &record)?;
    Ok(PreparedBinding {
        record,
        record_path,
        verified,
    })
}

// ---------------------------------------------------------------------------
// Apply phase
// ---------------------------------------------------------------------------

/// Loads the retained prepare-phase binding artifact of a durable attempt, if
/// the attempt carries one. The attempt loader has already re-verified the
/// artifact digest against the attempt manifest.
pub(crate) fn load_retained_binding_from(
    root: &Path,
    store: Option<&Path>,
    attempt_id: &crate::app::repair_attempt::RepairAttemptId,
) -> Result<Option<RetainedBinding>, String> {
    let manifest =
        crate::app::repair_attempt::load_repair_attempt_manifest_from(root, store, attempt_id)?;
    let Some(artifact) = crate::app::repair_attempt::find_manifest_artifact_by_role(
        &manifest,
        BINDING_ARTIFACT_ROLE,
    ) else {
        return Ok(None);
    };
    let bytes = std::fs::read(root.join(&artifact.path)).map_err(|error| {
        format!(
            "read retained python repair-trust binding {} failed: {error}",
            artifact.path
        )
    })?;
    let text = String::from_utf8(bytes)
        .map_err(|error| format!("retained python repair-trust binding is not UTF-8: {error}"))?;
    let value = parse_strict_json(&text).map_err(|error| {
        format!("retained python repair-trust binding is not well-formed JSON: {error}")
    })?;
    Ok(Some(RetainedBinding {
        artifact_sha256: artifact.sha256.trim_start_matches("sha256:").to_string(),
        value,
    }))
}

/// Re-verifies the retained binding immediately before the after phase records
/// an applied edit: the selection manifest must still digest to the recorded
/// value, the row must still digest to its recorded `selection_digest`, the
/// target and packet identities must be unchanged, and the invocation must
/// re-affirm the retained authorization with the same authority. Any drift
/// fails before `finish` runs, so a replaced selection or a changed manifest
/// can never be recorded as an applied edit.
pub(crate) fn reverify_for_apply(
    seam_id: &str,
    policy: &EditCagePolicy,
    packet_bytes: &[u8],
    retained: &RetainedBinding,
    authorization: &EditAuthorization,
) -> Result<VerifiedSelection, String> {
    let authority = authorization.verify("record the applied edit")?;
    let record = as_object(&retained.value, "retained binding record")?;
    reject_unknown_keys(record, &PREPARE_RECORD_KEYS, "retained binding record")?;
    for (field, expected) in [
        ("schema_version", BINDING_SCHEMA_VERSION),
        ("kind", BINDING_KIND),
        ("spec", BINDING_SPEC),
        ("phase", "prepare"),
    ] {
        let actual = require_string("retained binding record", record, field)?;
        if actual != expected {
            return Err(format!(
                "retained binding record field `{field}` must be `{expected}` for the apply phase, got `{actual}`"
            ));
        }
    }
    if record.contains_key("apply") {
        return Err(
            "retained binding record already carries an apply block; a prepare record cannot be replayed as an apply"
                .to_string(),
        );
    }
    let record_seam = require_string("retained binding record", record, "seam_id")?;
    if record_seam != seam_id {
        return Err(format!(
            "retained binding record names seam `{record_seam}` but the selected attempt belongs to seam `{seam_id}`"
        ));
    }

    let retained_authorization = as_object(
        record
            .get("authorization")
            .ok_or_else(|| "retained binding record is missing authorization".to_string())?,
        "retained authorization",
    )?;
    reject_unknown_keys(
        retained_authorization,
        &AUTHORIZATION_KEYS,
        "retained authorization",
    )?;
    let retained_status =
        require_string("retained authorization", retained_authorization, "status")?;
    if retained_status != AUTHORIZATION_STATUS {
        return Err(format!(
            "retained binding authorization status is `{retained_status}`; the driver records no applied edit without explicit operator or agent authorization"
        ));
    }
    let retained_authority = require_string(
        "retained authorization",
        retained_authorization,
        "authority",
    )?;
    if retained_authority != authority {
        return Err(format!(
            "apply authorization authority `{authority}` does not match the retained authorization authority; a different authority requires a new re-authorized attempt"
        ));
    }
    let retained_method =
        require_string("retained authorization", retained_authorization, "method")?;
    if retained_method != AUTHORIZATION_METHOD {
        return Err(format!(
            "retained binding authorization method `{retained_method}` is not the explicit-operator-flags authority"
        ));
    }

    // The closed nested blocks are shape-checked on the retained record so a
    // tampered or future-schema record cannot slip a field past the digest
    // anchors. Where the emitted shape is known, the VALUES are validated too
    // (digest-shaped digests, non-empty strings, portable edit-surface paths),
    // consistent with the binding checks the prepare phase ran on the same
    // fields. Two fields stay deliberately shape-only: `driver.binary_sha256`
    // is provenance, not a pin (the driver binary may be rebuilt between the
    // phases, so it is never compared to the running binary), and
    // `input.before_snapshot_sha256` binds its bytes through the durable
    // attempt loader's manifest artifact digests, which re-verify the staged
    // file before this function runs.
    let nested_blocks: [(&str, &str, &[&str]); 4] = [
        ("driver", "retained binding driver", &DRIVER_KEYS),
        ("config", "retained binding config", &CONFIG_KEYS),
        ("input", "retained binding input", &INPUT_KEYS),
        (
            "edit_surface",
            "retained binding edit surface",
            &EDIT_SURFACE_KEYS,
        ),
    ];
    for (field, subject, allowed) in nested_blocks {
        let block = as_object(
            record
                .get(field)
                .ok_or_else(|| format!("retained binding record is missing {field}"))?,
            subject,
        )?;
        reject_unknown_keys(block, allowed, subject)?;
        match field {
            "driver" => {
                let binary = require_string(subject, block, "binary_sha256")?;
                if !is_sha256_hex(&binary) {
                    return Err(format!(
                        "python repair-trust binding: subject=`{subject}` field=`binary_sha256`: must be bare lowercase sha256 hex"
                    ));
                }
                require_string(subject, block, "version")?;
            }
            "config" => {
                require_string(subject, block, "profile")?;
            }
            "input" => {
                for digest_field in ["packet_sha256", "before_snapshot_sha256"] {
                    let digest = require_string(subject, block, digest_field)?;
                    if !is_sha256_hex(&digest) {
                        return Err(format!(
                            "python repair-trust binding: subject=`{subject}` field=`{digest_field}`: must be bare lowercase sha256 hex"
                        ));
                    }
                }
            }
            "edit_surface" => {
                for surface_field in ["allowed", "forbidden"] {
                    let paths = match block.get(surface_field) {
                        Some(Value::Array(values)) => values,
                        _ => {
                            return Err(format!(
                                "python repair-trust binding: subject=`{subject}` field=`{surface_field}`: must be an array of paths"
                            ));
                        }
                    };
                    if surface_field == "allowed" && paths.is_empty() {
                        return Err(format!(
                            "python repair-trust binding: subject=`{subject}` field=`allowed`: must name at least one allowed path"
                        ));
                    }
                    for (index, value) in paths.iter().enumerate() {
                        let path = value.as_str().ok_or_else(|| {
                            format!(
                                "python repair-trust binding: subject=`{subject}` field=`{surface_field}[{index}]`: path must be a string"
                            )
                        })?;
                        check_portable_path(path).map_err(|error| {
                            format!(
                                "python repair-trust binding: subject=`{subject}` field=`{surface_field}[{index}]`: {error}"
                            )
                        })?;
                    }
                }
            }
            _ => {
                // Unreachable: the block list above is closed.
            }
        }
    }
    let record_non_claims = match record.get("non_claims") {
        Some(Value::Array(values)) => values,
        _ => return Err("retained binding record must carry its non_claims".to_string()),
    };
    // The standing claim boundary must ride verbatim: every element must be a
    // string and the collection must be exactly the standing list — a dropped
    // non-claim weakens the boundary and an extra string can smuggle a claim.
    let mut recorded_non_claims = BTreeSet::new();
    for (index, value) in record_non_claims.iter().enumerate() {
        let text = value.as_str().ok_or_else(|| {
            format!("retained binding record non_claims[{index}]: non-claim must be a string")
        })?;
        recorded_non_claims.insert(text);
    }
    let expected_non_claims: BTreeSet<&str> = BINDING_NON_CLAIMS.into_iter().collect();
    if recorded_non_claims != expected_non_claims {
        return Err(format!(
            "retained binding record must carry exactly the standing non-claims {BINDING_NON_CLAIMS:?}; a dropped or extra entry fails"
        ));
    }

    let trust = as_object(
        record
            .get("trust")
            .ok_or_else(|| "retained binding record is missing trust".to_string())?,
        "retained binding trust",
    )?;
    reject_unknown_keys(trust, &TRUST_KEYS, "retained binding trust")?;
    let manifest_path_string = require_string(
        "retained binding record",
        record,
        TELEMETRY_MANIFEST_PATH_FIELD,
    )?;
    let request = PythonRepairTrustSelection {
        manifest_path: PathBuf::from(&manifest_path_string),
        attempt_id: require_string("retained binding trust", trust, "attempt_id")?,
    };
    let (manifest_sha256, rows) =
        load_selection_manifest(&request.manifest_path).map_err(|error| {
            format!("stale packet rejected before recording the applied edit: {error}")
        })?;
    let recorded_manifest_sha256 =
        require_string("retained binding trust", trust, "selection_manifest_sha256")?;
    if recorded_manifest_sha256 != manifest_sha256 {
        return Err(format!(
            "stale selection manifest: the retained binding pins manifest sha256 `{recorded_manifest_sha256}` but {} now digests to `{manifest_sha256}`; a changed manifest requires a new re-authorized attempt",
            request.manifest_path.display()
        ));
    }
    let row = rows
        .iter()
        .find(|row| {
            row.get("attempt_id")
                .and_then(Value::as_str)
                .map(|value| value == request.attempt_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            format!(
                "selection attempt `{}` no longer exists in the accepted selection manifest; selected rows cannot be deleted or replaced after outcome",
                request.attempt_id
            )
        })?;
    // The apply phase deliberately does NOT re-check the row's head pin
    // against the current HEAD: repository drift between prepare and apply is
    // owned by the durable attempt authority, whose finish records the typed
    // `stale` state instead of silently refusing. Binding integrity (digests,
    // target alignment, packet identity, authorization) is what must fail
    // before the applied edit is recorded.
    let mut verified = verify_row_binding(row, &request.attempt_id).map_err(|error| {
        format!("stale packet rejected before recording the applied edit: {error}")
    })?;
    verified.selection_manifest_sha256 = manifest_sha256;
    verified.selection_manifest_path = manifest_path_string;
    verify_row_target_alignment(&verified, policy).map_err(|error| {
        format!("stale packet rejected before recording the applied edit: {error}")
    })?;

    let input = as_object(
        record
            .get("input")
            .ok_or_else(|| "retained binding record is missing input".to_string())?,
        "retained binding input",
    )?;
    reject_unknown_keys(input, &INPUT_KEYS, "retained binding input")?;
    let retained_packet_sha256 = require_string("retained binding input", input, "packet_sha256")?;
    let packet_sha256 = sha256_hex(packet_bytes);
    if retained_packet_sha256 != packet_sha256 {
        return Err(format!(
            "stale packet: the retained binding pins packet sha256 `{retained_packet_sha256}` but the attempt's retained packet digests to `{packet_sha256}`"
        ));
    }
    Ok(verified)
}

/// The apply record's digest chain: the claimed `binding_artifact_sha256`
/// must be the retained prepare artifact's staged digest, which the durable
/// attempt authority byte-verifies against the staged file. A claimed digest
/// that leaves the retained prepare artifact is a fabricated chain and fails.
fn check_binding_artifact_chain(staged_sha256_prefixed: &str, claimed: &str) -> Result<(), String> {
    let staged = staged_sha256_prefixed.trim_start_matches("sha256:");
    if !is_sha256_hex(staged) || staged != claimed {
        return Err(format!(
            "apply record refuses a binding-artifact digest that does not match the retained prepare artifact (staged `{staged_sha256_prefixed}`)"
        ));
    }
    Ok(())
}

/// The late-window manifest confirmation distinguishes a replaced manifest —
/// a deliberate named refusal that maps to the decision exit code 3 — from
/// operational failures reading the retained binding or the manifest itself,
/// which map to exit code 2.
pub(crate) enum ManifestConfirmationError {
    /// The manifest digests differently from the retained binding's pinned
    /// digest: replaced trust data refuses the finish.
    Replaced(String),
    /// The retained binding or the manifest could not be read or validated.
    Operational(String),
}

impl ManifestConfirmationError {
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Replaced(message) | Self::Operational(message) => message,
        }
    }
}

impl From<ManifestConfirmationError> for String {
    fn from(error: ManifestConfirmationError) -> Self {
        error.message().to_string()
    }
}

impl std::fmt::Display for ManifestConfirmationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

/// Re-reads the selection manifest at its recorded telemetry path and
/// requires the pinned digest. This closes the late publication window: the
/// apply verification runs before several expensive after-phase operations,
/// so the exact manifest bytes are confirmed again immediately before the
/// durable attempt advances. A manifest replaced inside that window refuses
/// here, leaving the attempt `awaiting_edit` instead of recording an edit
/// against silently replaced trust data.
pub(crate) fn confirm_manifest_unchanged(
    retained: &RetainedBinding,
) -> Result<(), ManifestConfirmationError> {
    let record = as_object(&retained.value, "retained binding record")
        .map_err(ManifestConfirmationError::Operational)?;
    let trust = record
        .get("trust")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ManifestConfirmationError::Operational(
                "retained binding record is missing trust".to_string(),
            )
        })?;
    let pinned = require_string("retained binding trust", trust, "selection_manifest_sha256")
        .map_err(ManifestConfirmationError::Operational)?;
    let path = require_string(
        "retained binding record",
        record,
        TELEMETRY_MANIFEST_PATH_FIELD,
    )
    .map_err(ManifestConfirmationError::Operational)?;
    let (current, _) = load_selection_manifest(Path::new(&path)).map_err(|error| {
        ManifestConfirmationError::Operational(format!(
            "stale packet rejected before the durable finish: {error}"
        ))
    })?;
    if current != pinned {
        return Err(ManifestConfirmationError::Replaced(format!(
            "stale selection manifest: the retained binding pins manifest sha256 `{pinned}` but {path} now digests to `{current}`; a changed manifest requires a new re-authorized attempt"
        )));
    }
    Ok(())
}

/// Renders and publishes the apply-phase record after the durable attempt
/// recorded its after verdict. The record carries the changed-file set, the
/// patch digest, the edit-cage decision, and the resulting repository head —
/// and no verification, movement, or closure claim. The identities are read
/// from the durable attempt's own retained artifacts, so the record restates
/// the authority instead of re-deriving it.
pub(crate) fn write_apply_record_from(
    root: &Path,
    store: Option<&Path>,
    attempt_id: &crate::app::repair_attempt::RepairAttemptId,
    retained_artifact_sha256: &str,
    verified: &VerifiedSelection,
    authority: &str,
    after: &crate::app::repair_attempt::RepairAttemptAfter,
) -> Result<PathBuf, String> {
    let manifest =
        crate::app::repair_attempt::load_repair_attempt_manifest_from(root, store, attempt_id)?;
    let policy = crate::app::repair_attempt::load_edit_cage_policy_from(root, store, attempt_id)?;
    // The digest chain is verified against the staged prepare artifact before
    // anything is rendered: a claimed digest that leaves the retained binding
    // fails here instead of being published into the record.
    let binding_artifact = crate::app::repair_attempt::find_manifest_artifact_by_role(
        &manifest,
        BINDING_ARTIFACT_ROLE,
    )
    .ok_or_else(|| {
        "durable attempt is missing its retained python repair-trust binding".to_string()
    })?;
    let binding_bytes = std::fs::read(root.join(&binding_artifact.path)).map_err(|error| {
        format!(
            "read retained python repair-trust binding {} failed: {error}",
            binding_artifact.path
        )
    })?;
    if sha256_hex(&binding_bytes) != binding_artifact.sha256.trim_start_matches("sha256:") {
        return Err(
            "apply record refuses a retained prepare artifact whose bytes leave its digest"
                .to_string(),
        );
    }
    check_binding_artifact_chain(&binding_artifact.sha256, retained_artifact_sha256)?;
    let packet_artifact =
        crate::app::repair_attempt::find_manifest_artifact_by_role(&manifest, "agent_packet")
            .ok_or_else(|| "durable attempt is missing its retained agent packet".to_string())?;
    let before_artifact =
        crate::app::repair_attempt::find_manifest_artifact_by_role(&manifest, "before_snapshot")
            .ok_or_else(|| "durable attempt is missing its retained before snapshot".to_string())?;
    let packet_sha256 = packet_artifact
        .sha256
        .trim_start_matches("sha256:")
        .to_string();
    let before_snapshot_sha256 = before_artifact
        .sha256
        .trim_start_matches("sha256:")
        .to_string();
    if !is_sha256_hex(&packet_sha256) || !is_sha256_hex(&before_snapshot_sha256) {
        return Err("apply record refuses malformed durable attempt digests".to_string());
    }
    let patch_sha256 = after.delta_sha256.trim_start_matches("sha256:").to_string();
    if !is_sha256_hex(&patch_sha256) {
        return Err("apply record refuses a malformed patch digest".to_string());
    }
    if after.packet_sha256 != packet_artifact.sha256 {
        return Err(
            "apply record refuses an after verdict whose packet digest leaves the retained artifacts"
                .to_string(),
        );
    }
    let cage_status = serde_json::to_value(after.verdict.status)
        .map_err(|error| format!("serialize edit-cage status failed: {error}"))?;
    let changed_paths = after
        .verdict
        .changed_paths
        .iter()
        .map(|path| path.as_str())
        .collect::<Vec<_>>();
    let apply = serde_json::json!({
        "patch_sha256": patch_sha256,
        "changed_paths": changed_paths,
        "cage_status": cage_status,
        "repository_head_after": after.repository_head,
        "current": after.current,
    });
    let record = render_record(
        root,
        RecordIdentity {
            seam_id: &manifest.seam_id,
            repository_head: &verified.head,
            phase: "apply",
            durable_attempt_id: Some(attempt_id.as_str()),
            binding_artifact_sha256: Some(retained_artifact_sha256),
            verified,
            authority,
            packet_sha256: &packet_sha256,
            before_snapshot_sha256: &before_snapshot_sha256,
            policy: &policy,
        },
        Some(apply),
    )?;
    let record_path = root.join(APPLY_RECORD_COMPAT_PATH);
    write_record_file(&record_path, &record)?;
    Ok(record_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The authority identity the retained-record fixture grants.
    const AUTHORITY_IDENTITY: &str = "test-operator";

    fn sample_row() -> Result<Value, String> {
        let mut value: Value = serde_json::from_str(
            r#"{
            "attempt_id": "att-test-1",
            "case_id": "case-1",
            "subject_id": "subj-1",
            "repository": "https://example.com/subj-1",
            "base": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "head": "cccccccccccccccccccccccccccccccccccccccc",
            "selection_reason": "behavior changed in the diff",
            "diversity_stratum": "pytest_library",
            "family": "error_path_gating",
            "owner": "pricing.calculate_discount",
            "discriminator": "amount == threshold",
            "relation": "test calls owner directly",
            "oracle": "assert exact boundary value",
            "expected_direction": "should_gap",
            "claim_boundary": "static exposure evidence only",
            "target_path": "tests/test_pricing.py",
            "target_state": "existing",
            "selected_at": "2026-09-10T00:00:00Z",
            "selector": "campaign-selector",
            "authority_snapshot_digest": "2020202020202020202020202020202020202020202020202020202020202020",
            "selection_digest": "d"
        }"#,
        )
        .map_err(|error| format!("sample row json: {error}"))?;
        {
            let object = value
                .as_object_mut()
                .ok_or_else(|| "sample row is not an object".to_string())?;
            let digest = canonical_selection_digest(object)?;
            object.insert(SELECTION_DIGEST_FIELD.to_string(), Value::String(digest));
        }
        Ok(value)
    }

    #[test]
    fn canonical_digest_is_stable_and_matches_the_corpus_preimage() -> Result<(), String> {
        let row = sample_row()?;
        let object = as_object(&row, "sample row")?;
        let digest = canonical_selection_digest(object)?;
        let again = canonical_selection_digest(object)?;
        if digest != again || digest.len() != 64 {
            return Err("canonical selection digest was not stable 64-hex".to_string());
        }
        // Recomputing the preimage independently: the row without its digest
        // field, serialized with sorted keys.
        let mut manual = object.clone();
        manual.remove(SELECTION_DIGEST_FIELD);
        let expected = sha256_hex(
            serde_json::to_string(&Value::Object(manual))
                .map_err(|error| error.to_string())?
                .as_bytes(),
        );
        if digest != expected {
            return Err("canonical selection digest left the documented preimage".to_string());
        }
        Ok(())
    }

    #[test]
    fn denied_edit_surface_vocabulary_rejects_generated_and_environment_paths() -> Result<(), String>
    {
        // The prefix matcher covers the generated/vendor/environment/cache
        // vocabulary; a production path such as `src/pricing.py` is not a
        // denial-family match and is refused earlier, by the positive
        // test-surface gate on the packet's selected edit target
        // (`edit_cage_policy_from_packet`) plus the `existing` target-state
        // requirement and the packet's forbidden-path rules, exactly as the
        // corpus declares it `unsafe`.
        for allowed in ["tests/test_pricing.py", "tests/unit/test_x.py"] {
            if is_denied_edit_surface(allowed) {
                return Err(format!("test path `{allowed}` was denied"));
            }
        }
        for denied in [
            "generated/module.py",
            "vendor/lib.py",
            "vendored/lib.py",
            "dist/bundle.py",
            "build/out.py",
            "target/cache/x.py",
            "node_modules/pkg/mod.py",
            "__pycache__/mod.py",
            "site-packages/pkg.py",
            ".venv/lib.py",
            "env/lib.py",
            ".tox/py/lib.py",
            "src/foo.generated.py",
        ] {
            if !is_denied_edit_surface(denied) {
                return Err(format!("denied surface `{denied}` was accepted"));
            }
        }
        Ok(())
    }

    #[test]
    fn strict_json_rejects_duplicate_keys() -> Result<(), String> {
        if parse_strict_json("{\"a\":1,\"a\":2}").is_ok() {
            return Err("duplicate top-level key was accepted".to_string());
        }
        if parse_strict_json("{\"a\":{\"b\":1,\"b\":2}}").is_ok() {
            return Err("duplicate nested key was accepted".to_string());
        }
        let value = parse_strict_json("{\"a\":1,\"b\":[2,null,true]}")
            .map_err(|error| format!("well-formed JSON was rejected: {error}"))?;
        if value.get("a").and_then(Value::as_i64) != Some(1) {
            return Err("strict JSON round trip changed a value".to_string());
        }
        Ok(())
    }

    #[test]
    fn portable_path_check_rejects_escape_and_absolute_forms() -> Result<(), String> {
        check_portable_path("tests/test_x.py").map_err(|error| error.clone())?;
        // The drive letter is built at runtime so no local absolute path is
        // committed as a literal.
        let drive = char::from(b'C');
        let drive_back = format!("{drive}:\\tmp.py");
        let drive_forward = format!("{drive}:/tmp.py");
        for rejected in [
            "../outside.py",
            "a/../../outside.py",
            "/abs.py",
            drive_back.as_str(),
            drive_forward.as_str(),
            "a\\b.py",
            "",
        ] {
            if check_portable_path(rejected).is_ok() {
                return Err(format!("portable path check accepted `{rejected}`"));
            }
        }
        Ok(())
    }

    #[test]
    fn authorization_refusal_names_the_missing_signals() -> Result<(), String> {
        let refused = EditAuthorization {
            authorized: false,
            authority: None,
        };
        let error = match refused.verify("prepare the edit transaction") {
            Err(error) => error,
            Ok(_) => return Err("unauthorized prepare was accepted".to_string()),
        };
        for signal in ["--edit-authorized", "--edit-authority"] {
            if !error.contains(signal) {
                return Err(format!("refusal did not name `{signal}`: {error}"));
            }
        }
        let flag_only = EditAuthorization {
            authorized: true,
            authority: None,
        };
        let error = match flag_only.verify("record the applied edit") {
            Err(error) => error,
            Ok(_) => return Err("authorization without an authority was accepted".to_string()),
        };
        if !error.contains("--edit-authority") {
            return Err(format!(
                "refusal did not name the authority signal: {error}"
            ));
        }
        Ok(())
    }

    #[test]
    fn digest_shape_checks_reject_prefixed_and_malformed_values() -> Result<(), String> {
        if is_sha256_hex("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        {
            return Err("prefixed digest passed the corpus-shape check".to_string());
        }
        if is_sha256_hex("0123") {
            return Err("short digest passed the corpus-shape check".to_string());
        }
        if !is_sha256_hex(&"a".repeat(64)) {
            return Err("64-hex digest failed the corpus-shape check".to_string());
        }
        if is_git_sha("GGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG") {
            return Err("non-hex commit SHA passed the git-shape check".to_string());
        }
        if !is_git_sha(&"a".repeat(40)) {
            return Err("40-hex commit SHA failed the git-shape check".to_string());
        }
        Ok(())
    }

    #[test]
    fn normalized_target_paths_bind_identically() -> Result<(), String> {
        // `./tests/x.rs` and `tests//x.rs` bind identically to `tests/x.rs`:
        // the row digest covers the raw accepted bytes, and the bound target
        // is the normalized spelling.
        for spelling in ["./tests/test_pricing.py", "tests//test_pricing.py"] {
            let mut row = sample_row()?;
            {
                let object = row
                    .as_object_mut()
                    .ok_or_else(|| "sample row is not an object".to_string())?;
                object.insert(
                    "target_path".to_string(),
                    Value::String(spelling.to_string()),
                );
                let digest = canonical_selection_digest(object)?;
                object.insert(SELECTION_DIGEST_FIELD.to_string(), Value::String(digest));
            }
            let object = as_object(&row, "sample row")?;
            let verified = verify_row_binding(object, "att-test-1")
                .map_err(|error| format!("spelling `{spelling}` was refused: {error}"))?;
            if verified.target_path != "tests/test_pricing.py" {
                return Err(format!(
                    "raw spelling `{spelling}` was not normalized: `{}`",
                    verified.target_path
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn identical_record_inputs_serialize_identically() -> Result<(), String> {
        // This is deterministic construction proof, not selection admission.
        // The packet bytes are a committed producer snapshot, not a digest
        // placeholder. Neither call creates or publishes a durable attempt.
        let packet = include_bytes!(
            "../../../../fixtures/boundary_gap/expected/editor-agent-loop/agent-packet.json"
        );
        serde_json::from_slice::<Value>(packet)
            .map_err(|error| format!("producer packet fixture is invalid: {error}"))?;
        let packet_sha256 = sha256_hex(packet);
        let before_snapshot_sha256 = sha256_hex(b"identical before snapshot bytes");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/boundary_gap");
        let row = sample_row()?;
        let verified = verify_row_binding(as_object(&row, "selection row")?, "att-test-1")?;
        let target = crate::edit_cage::CagePathRule::exact(&verified.target_path)?;
        let policy = EditCagePolicy {
            selected_target: target.clone(),
            allowed_edit_surface: vec![target],
            forbidden_paths: Vec::new(),
            expected_operational_writes: Vec::new(),
            ignored_build_output: None,
            untracked_build_lockfile: None,
            inline_test_module_target: false,
        };
        let render = || {
            render_record(
                &root,
                RecordIdentity {
                    seam_id: "identical-seam",
                    repository_head: &verified.head,
                    phase: "prepare",
                    durable_attempt_id: None,
                    binding_artifact_sha256: None,
                    verified: &verified,
                    authority: AUTHORITY_IDENTITY,
                    packet_sha256: &packet_sha256,
                    before_snapshot_sha256: &before_snapshot_sha256,
                    policy: &policy,
                },
                None,
            )
        };
        let first = render()?;
        let second = render()?;
        if first
            .pointer("/input/packet_sha256")
            .and_then(Value::as_str)
            != Some(packet_sha256.as_str())
        {
            return Err("record lost the identical producer packet digest".to_string());
        }
        let first_bytes = serde_json::to_vec(&first).map_err(|error| error.to_string())?;
        let second_bytes = serde_json::to_vec(&second).map_err(|error| error.to_string())?;
        if first_bytes != second_bytes {
            return Err("identical accepted record inputs serialized differently".to_string());
        }
        Ok(())
    }

    /// A dangling `ripr.toml` is present: the binding profile must match
    /// config discovery, not report built-in defaults.
    #[cfg(unix)]
    #[test]
    fn dangling_ripr_toml_symlink_is_subject_config_not_defaults() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("clock: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-python-repair-binding-dangling-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        if detect_config_profile(&root) != "default" {
            let _ = std::fs::remove_dir_all(&root);
            return Err("an absent ripr.toml must stay the default profile".to_string());
        }
        std::os::unix::fs::symlink("no-such-target.toml", root.join("ripr.toml"))
            .map_err(|error| error.to_string())?;
        let load_error = match crate::config::load_for_root(&root) {
            Ok(_) => {
                let _ = std::fs::remove_dir_all(&root);
                return Err("load_for_root must refuse a dangling ripr.toml".to_string());
            }
            Err(error) => error,
        };
        let profile = detect_config_profile(&root);
        let _ = std::fs::remove_dir_all(&root);
        if !load_error.contains("ripr.toml") {
            return Err(format!(
                "load_for_root must name ripr.toml for a dangling link: {load_error}"
            ));
        }
        if profile != "subject-ripr-toml" {
            return Err(format!(
                "a dangling ripr.toml must be subject-ripr-toml, not {profile}"
            ));
        }
        Ok(())
    }

    /// A minimal retained prepare record whose early re-verification checks
    /// pass, so the bounded value validation of the nested blocks is the first
    /// stage under test. The trust block is a stub: value tampering fails
    /// before the telemetry manifest is ever read.
    fn retained_record_fixture() -> Value {
        serde_json::json!({
            "schema_version": BINDING_SCHEMA_VERSION,
            "kind": BINDING_KIND,
            "spec": BINDING_SPEC,
            "phase": "prepare",
            "seam_id": "seam-under-test",
            "repository_head": "cccccccccccccccccccccccccccccccccccccccc",
            "selection_manifest_path": "unused-manifest.json",
            "driver": {
                "binary_sha256": sha256_hex(b"driver binary bytes"),
                "version": "0.11.0",
            },
            "config": {
                "profile": "subject-ripr-toml",
            },
            "input": {
                "packet_sha256": sha256_hex(b"packet bytes"),
                "before_snapshot_sha256": sha256_hex(b"before snapshot bytes"),
            },
            "trust": {
                "attempt_id": "att-reverify",
            },
            "edit_surface": {
                "allowed": ["tests/test_pricing.py"],
                "forbidden": [],
            },
            "authorization": {
                "status": AUTHORIZATION_STATUS,
                "authority": AUTHORITY_IDENTITY,
                "method": AUTHORIZATION_METHOD,
            },
            "non_claims": BINDING_NON_CLAIMS,
        })
    }

    #[test]
    fn reverify_rejects_tampered_retained_values() -> Result<(), String> {
        let authorization = EditAuthorization {
            authorized: true,
            authority: Some(AUTHORITY_IDENTITY.to_string()),
        };
        let policy = EditCagePolicy {
            selected_target: crate::edit_cage::CagePathRule::exact("tests/test_pricing.py")?,
            allowed_edit_surface: vec![crate::edit_cage::CagePathRule::exact(
                "tests/test_pricing.py",
            )?],
            forbidden_paths: Vec::new(),
            expected_operational_writes: vec![crate::edit_cage::CagePathRule::subtree(
                "target/ripr",
            )?],
            ignored_build_output: None,
            untracked_build_lockfile: None,
            inline_test_module_target: false,
        };
        // Positive control: with intact values the bounded value stage passes
        // and verification proceeds to the telemetry manifest, which does not
        // exist in the test environment — the failure must name the manifest,
        // proving the record reached the later stage.
        let intact = RetainedBinding {
            artifact_sha256: String::new(),
            value: retained_record_fixture(),
        };
        let error = match reverify_for_apply(
            "seam-under-test",
            &policy,
            b"packet bytes",
            &intact,
            &authorization,
        ) {
            Err(error) => error,
            Ok(_) => return Err("a record naming no manifest passed re-verification".to_string()),
        };
        if !error.contains("selection manifest") {
            return Err(format!(
                "intact retained values did not reach the manifest stage: {error}"
            ));
        }
        // Bounded value tampering is detected at the value stage, before any
        // manifest is read.
        let cases: [(&str, Value, &str); 6] = [
            (
                "driver.binary_sha256",
                serde_json::json!("not-a-digest"),
                "must be bare lowercase sha256 hex",
            ),
            ("driver.version", serde_json::json!(""), "must be non-empty"),
            ("config.profile", serde_json::json!(""), "must be non-empty"),
            (
                "input.packet_sha256",
                serde_json::json!("0123"),
                "must be bare lowercase sha256 hex",
            ),
            (
                "edit_surface.allowed",
                serde_json::json!([]),
                "must name at least one allowed path",
            ),
            (
                "edit_surface.forbidden",
                serde_json::json!(["../escape.py"]),
                "must not contain `..` components",
            ),
        ];
        for (field, replacement, needle) in cases {
            let mut record = retained_record_fixture();
            let object = record
                .as_object_mut()
                .ok_or_else(|| "retained fixture is not an object".to_string())?;
            let (block_name, key) = field
                .split_once('.')
                .ok_or_else(|| format!("test case `{field}` does not name a nested block field"))?;
            let block = object
                .get_mut(block_name)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| format!("retained fixture is missing `{block_name}`"))?;
            block.insert(key.to_string(), replacement);
            let tampered = RetainedBinding {
                artifact_sha256: String::new(),
                value: record,
            };
            let error = match reverify_for_apply(
                "seam-under-test",
                &policy,
                b"packet bytes",
                &tampered,
                &authorization,
            ) {
                Err(error) => error,
                Ok(_) => {
                    return Err(format!(
                        "tampered retained `{field}` passed re-verification"
                    ));
                }
            };
            if !error.contains(needle) || !error.contains(&format!("field=`{key}")) {
                return Err(format!(
                    "tampered retained `{field}` was not the typed failure (`{needle}`): {error}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn binding_artifact_chain_refuses_fabricated_digests() -> Result<(), String> {
        // The staged digest of a real artifact bytes passes the chain.
        let staged = format!("sha256:{}", sha256_hex(b"retained prepare artifact bytes"));
        check_binding_artifact_chain(&staged, staged.trim_start_matches("sha256:"))
            .map_err(|error| format!("a real staged digest was refused: {error}"))?;
        // A fabricated 64-hex digest that no retained artifact carries fails.
        let fabricated = "0".repeat(64);
        let error = match check_binding_artifact_chain(&staged, &fabricated) {
            Err(error) => error,
            Ok(()) => return Err("a fabricated binding digest passed the chain".to_string()),
        };
        if !error.contains("does not match the retained prepare artifact") {
            return Err(format!("unexpected chain refusal: {error}"));
        }
        // A malformed staged digest fails too.
        if check_binding_artifact_chain("sha256:0123", &"0".repeat(64)).is_ok() {
            return Err("a malformed staged digest passed the chain".to_string());
        }
        Ok(())
    }

    #[test]
    fn confirm_manifest_unchanged_detects_a_replaced_manifest() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("test clock failed: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-binding-confirm-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("create temp dir failed: {error}"))?;
        let result = (|| -> Result<(), String> {
            let manifest_path = root.join("trust-manifest.json");
            let manifest = serde_json::json!({
                "schema_version": MANIFEST_SCHEMA_VERSION,
                "kind": MANIFEST_KIND,
                "spec": BINDING_SPEC,
                "description": "confirm fixture",
                "selections": [],
            });
            let text = serde_json::to_string_pretty(&manifest)
                .map_err(|error| format!("serialize manifest: {error}"))?;
            std::fs::write(&manifest_path, &text)
                .map_err(|error| format!("write manifest: {error}"))?;
            let pinned = sha256_hex(text.as_bytes());
            let retained = RetainedBinding {
                artifact_sha256: pinned.clone(),
                // The telemetry path field is spelled literally because the
                // json! macro takes literal keys.
                value: serde_json::json!({
                    "trust": {
                        "selection_manifest_sha256": pinned,
                    },
                    "selection_manifest_path": manifest_path.display().to_string(),
                }),
            };
            confirm_manifest_unchanged(&retained)
                .map_err(|error| format!("intact manifest was refused: {error}"))?;

            // A manifest replaced inside the late publication window refuses.
            let replaced = serde_json::json!({
                "schema_version": MANIFEST_SCHEMA_VERSION,
                "kind": MANIFEST_KIND,
                "spec": BINDING_SPEC,
                "description": "replaced after prepare",
                "selections": [],
            });
            let replaced_text = serde_json::to_string_pretty(&replaced)
                .map_err(|error| format!("serialize replaced manifest: {error}"))?;
            std::fs::write(&manifest_path, &replaced_text)
                .map_err(|error| format!("write replaced manifest: {error}"))?;
            let message = match confirm_manifest_unchanged(&retained) {
                Err(ManifestConfirmationError::Replaced(message)) => message,
                Err(ManifestConfirmationError::Operational(message)) => {
                    return Err(format!("unexpected operational refusal: {message}"));
                }
                Ok(()) => {
                    return Err("a replaced manifest passed the late-window confirm".to_string());
                }
            };
            if !message.contains("stale selection manifest") {
                return Err(format!("unexpected confirm refusal: {message}"));
            }
            Ok(())
        })();
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove temp dir failed: {error}"))?;
        result
    }

    #[test]
    fn canonical_digest_is_order_independent() -> Result<(), String> {
        // The canonical preimage is built through a BTreeMap, so two rows
        // whose keys were inserted in different orders digest identically
        // regardless of any serde_json `preserve_order` feature state.
        let mut first = Map::new();
        first.insert("attempt_id".to_string(), Value::String("a".to_string()));
        first.insert("case_id".to_string(), Value::String("c".to_string()));
        let mut second = Map::new();
        second.insert("case_id".to_string(), Value::String("c".to_string()));
        second.insert("attempt_id".to_string(), Value::String("a".to_string()));
        let left = canonical_selection_digest(&first)?;
        let right = canonical_selection_digest(&second)?;
        if left != right {
            return Err("insertion order changed the canonical digest".to_string());
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Fail-closed refusal coverage for the bind, load, prepare and apply
    // re-verification paths. Every refusal case is paired with a positive
    // control, and each tampered input is rebuilt so the refusal can only
    // come from the check under test.
    // ------------------------------------------------------------------

    /// Removes its unique directory when the test ends, pass or fail.
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(tag: &str) -> Result<Self, String> {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("test clock failed: {error}"))?
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "ripr-python-repair-binding-{tag}-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path)
                .map_err(|error| format!("create {} failed: {error}", path.display()))?;
            Ok(Self(path))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn run_git(root: &Path, args: &[&str]) -> Result<(), String> {
        crate::testing::fixture_git::fixture_git_ok(root, args)
    }

    fn write_text(path: &Path, text: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
        }
        std::fs::write(path, text)
            .map_err(|error| format!("write {} failed: {error}", path.display()))
    }

    /// The sample row with `mutate` applied and its digest recomputed, so a
    /// refusal cannot be the digest-anchor check.
    fn redigested_row(mutate: impl FnOnce(&mut Map<String, Value>)) -> Result<Value, String> {
        let mut row = sample_row()?;
        let object = row
            .as_object_mut()
            .ok_or_else(|| "sample row is not an object".to_string())?;
        mutate(object);
        let digest = canonical_selection_digest(object)?;
        object.insert(SELECTION_DIGEST_FIELD.to_string(), Value::String(digest));
        Ok(row)
    }

    fn bind_error(row: &Value, attempt: &str) -> Result<String, String> {
        match verify_row_binding(as_object(row, "row")?, attempt) {
            Err(error) => Ok(error),
            Ok(_) => Err("a row that must be refused was bound".to_string()),
        }
    }

    fn manifest_text_of(rows: Vec<Value>) -> Result<String, String> {
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": MANIFEST_SCHEMA_VERSION,
            "kind": MANIFEST_KIND,
            "spec": BINDING_SPEC,
            "description": "unit fixture",
            "selections": rows,
        }))
        .map_err(|error| format!("serialize manifest: {error}"))
    }

    fn granted() -> EditAuthorization {
        EditAuthorization {
            authorized: true,
            authority: Some(AUTHORITY_IDENTITY.to_string()),
        }
    }

    fn policy_for(target: &str) -> Result<EditCagePolicy, String> {
        Ok(EditCagePolicy {
            selected_target: crate::edit_cage::CagePathRule::exact(target)?,
            allowed_edit_surface: vec![crate::edit_cage::CagePathRule::exact(target)?],
            forbidden_paths: Vec::new(),
            expected_operational_writes: Vec::new(),
            ignored_build_output: None,
            untracked_build_lockfile: None,
            inline_test_module_target: false,
        })
    }

    type Mutation = fn(&mut Map<String, Value>);

    #[test]
    fn row_binding_refuses_every_unbound_row_shape() -> Result<(), String> {
        // Positive control: the unmodified row binds and carries its identities.
        let sample = sample_row()?;
        let bound = verify_row_binding(as_object(&sample, "row")?, "att-test-1")?;
        if bound.target_path != "tests/test_pricing.py" || bound.target_state != "existing" {
            return Err(format!("control row bound the wrong target: {bound:?}"));
        }
        if bound.tree.is_some() || bound.source_currentness.is_some() || bound.limitation.is_some()
        {
            return Err("control row invented optional identities".to_string());
        }

        let cases: Vec<(&str, Mutation, &str)> = vec![
            (
                "target_state",
                |row| {
                    row.insert("target_state".into(), "proposed".into());
                },
                "declares target_state `proposed`",
            ),
            (
                "denied surface",
                |row| {
                    row.insert("target_path".into(), "vendor/test_x.py".into());
                },
                "falls under a production/generated/vendor/environment edit surface",
            ),
            (
                "escaping target",
                |row| {
                    row.insert("target_path".into(), "../tests/test_x.py".into());
                },
                "target_path: path `../tests/test_x.py` must not contain `..` components",
            ),
            (
                "short head",
                |row| {
                    row.insert("head".into(), "abc123".into());
                },
                "head must be a bare lowercase 40-character commit SHA",
            ),
            (
                "uppercase base",
                |row| {
                    row.insert("base".into(), "B".repeat(40).into());
                },
                "base must be a bare lowercase 40-character commit SHA",
            ),
            (
                "malformed tree",
                |row| {
                    row.insert("tree".into(), "not-a-digest".into());
                },
                "tree must be bare lowercase sha256 hex when present",
            ),
            (
                "empty limitation",
                |row| {
                    row.insert("limitation".into(), "  ".into());
                },
                "string field must be non-empty when present",
            ),
            (
                "numeric currentness",
                |row| {
                    row.insert("source_currentness".into(), serde_json::json!(7));
                },
                "field must be a string when present",
            ),
            (
                "missing owner",
                |row| {
                    row.remove("owner");
                },
                "field=`owner`: required field is missing",
            ),
            (
                "null oracle",
                |row| {
                    row.insert("oracle".into(), Value::Null);
                },
                "field=`oracle`: a present null is not a value",
            ),
            (
                "blank subject",
                |row| {
                    row.insert("subject_id".into(), "   ".into());
                },
                "field=`subject_id`: required field must be non-empty",
            ),
            (
                "numeric case",
                |row| {
                    row.insert("case_id".into(), serde_json::json!(1));
                },
                "field=`case_id`: field must be a string",
            ),
        ];
        for (label, mutate, needle) in cases {
            let row = redigested_row(mutate)?;
            if row == sample {
                return Err(format!("case `{label}` did not change the row"));
            }
            let error = bind_error(&row, "att-test-1")?;
            if !error.contains(needle) {
                return Err(format!(
                    "case `{label}` was not refused for `{needle}`: {error}"
                ));
            }
            if error.contains("digest mismatch") {
                return Err(format!(
                    "case `{label}` hit the digest anchor instead: {error}"
                ));
            }
        }

        // The optional identities are carried through when well formed.
        let tree = sha256_hex(b"tree bytes");
        let carried = redigested_row(|row| {
            row.insert("tree".into(), sha256_hex(b"tree bytes").into());
            row.insert("source_currentness".into(), "current".into());
            row.insert("limitation".into(), "static evidence only".into());
        })?;
        let verified = verify_row_binding(as_object(&carried, "row")?, "att-test-1")?;
        if verified.tree.as_deref() != Some(tree.as_str())
            || verified.source_currentness.as_deref() != Some("current")
            || verified.limitation.as_deref() != Some("static evidence only")
        {
            return Err(format!("optional identities were dropped: {verified:?}"));
        }
        Ok(())
    }

    #[test]
    fn row_binding_anchors_identity_and_digest_before_content() -> Result<(), String> {
        let sample = sample_row()?;
        let error = bind_error(&sample, "att-other")?;
        if !error.contains("does not match the requested attempt `att-other`") {
            return Err(format!(
                "substituted attempt was not refused by name: {error}"
            ));
        }
        // Editing a field after the digest was recorded is a replaced row.
        let mut edited = sample.clone();
        let object = edited
            .as_object_mut()
            .ok_or_else(|| "sample row is not an object".to_string())?;
        object.insert("oracle".into(), "assert anything".into());
        let error = bind_error(&edited, "att-test-1")?;
        if !error.contains("digest mismatch") || !error.contains("requires a new selection") {
            return Err(format!("edited row was not a digest mismatch: {error}"));
        }
        // A prefixed or uppercase recorded digest is not the corpus shape.
        for bad in [
            format!("sha256:{}", sha256_hex(b"x")),
            sha256_hex(b"x").to_ascii_uppercase(),
        ] {
            let mut shaped = sample.clone();
            let object = shaped
                .as_object_mut()
                .ok_or_else(|| "sample row is not an object".to_string())?;
            object.insert(SELECTION_DIGEST_FIELD.into(), bad.clone().into());
            let error = bind_error(&shaped, "att-test-1")?;
            if !error.contains("selection_digest must be bare lowercase sha256 hex") {
                return Err(format!("digest shape `{bad}` was not refused: {error}"));
            }
        }
        // A row with no recorded digest names the missing field.
        let mut missing = sample.clone();
        let object = missing
            .as_object_mut()
            .ok_or_else(|| "sample row is not an object".to_string())?;
        object.remove(SELECTION_DIGEST_FIELD);
        let error = bind_error(&missing, "att-test-1")?;
        if !error.contains("field=`selection_digest`: required field is missing") {
            return Err(format!("missing digest was not named: {error}"));
        }
        Ok(())
    }

    #[test]
    fn strict_accessors_distinguish_missing_null_blank_and_wrong_type() -> Result<(), String> {
        let value = serde_json::json!({"a": "x", "b": null, "c": "  ", "d": 3});
        let object = as_object(&value, "accessor fixture")?;
        if require_string("subject", object, "a")? != "x" {
            return Err("a present string was not returned".to_string());
        }
        for (field, needle) in [
            ("z", "required field is missing"),
            ("b", "a present null is not a value"),
            ("c", "required field must be non-empty"),
            ("d", "field must be a string"),
        ] {
            match require_string("subject", object, field) {
                Err(error) if error.contains(needle) && error.contains(field) => {}
                other => return Err(format!("field `{field}` expected `{needle}`: {other:?}")),
            }
        }
        if opt_string(object, "a")?.as_deref() != Some("x")
            || opt_string(object, "z")?.is_some()
            || opt_string(object, "b")?.is_some()
        {
            return Err("absent and null optionals must read as None".to_string());
        }
        for (field, needle) in [
            ("c", "must be non-empty when present"),
            ("d", "must be a string when present"),
        ] {
            match opt_string(object, field) {
                Err(error) if error.contains(needle) => {}
                other => {
                    return Err(format!("optional `{field}` expected `{needle}`: {other:?}"));
                }
            }
        }
        let typo = serde_json::json!({"a": 1, "typo": 2});
        let typo = as_object(&typo, "typo fixture")?;
        reject_unknown_keys(typo, &["a", "typo"], "typo fixture")?;
        match reject_unknown_keys(typo, &["a"], "typo fixture") {
            Err(error) if error.contains("unknown field `typo`") => {}
            other => return Err(format!("unknown key was not denied: {other:?}")),
        }
        let array = serde_json::json!([1]);
        match as_object(&array, "array fixture") {
            Err(error) if error.contains("subject=`array fixture`") => {}
            other => return Err(format!("a non-object was accepted: {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn authorization_trims_the_authority_and_refuses_blank_identities() -> Result<(), String> {
        let padded = EditAuthorization {
            authorized: true,
            authority: Some("  reviewer-1  ".to_string()),
        };
        if padded.verify("prepare the edit transaction")? != "reviewer-1" {
            return Err("the authority was not trimmed".to_string());
        }
        for blank in [Some("   ".to_string()), Some(String::new()), None] {
            let authorization = EditAuthorization {
                authorized: true,
                authority: blank.clone(),
            };
            match authorization.verify("record the applied edit") {
                Err(error) if error.contains("requires --edit-authority <identity>") => {}
                other => {
                    return Err(format!("blank authority {blank:?} was accepted: {other:?}"));
                }
            }
        }
        // An identity without the flag is still refused: both are required.
        let identity_only = EditAuthorization {
            authorized: false,
            authority: Some("reviewer-1".to_string()),
        };
        match identity_only.verify("prepare the edit transaction") {
            Err(error) if error.contains("--edit-authorized") => {}
            other => return Err(format!("an unflagged authority was accepted: {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn denied_surface_and_path_helpers_pin_their_boundaries() -> Result<(), String> {
        // Matching is case-insensitive and anchored to the path start.
        for denied in [
            "Vendor/Lib.py",
            "NODE_MODULES/x.py",
            "tests/gen.GENERATED.py",
        ] {
            if !is_denied_edit_surface(denied) {
                return Err(format!("`{denied}` escaped the denied vocabulary"));
            }
        }
        for allowed in [
            "src/vendor/lib.py",
            "environment/test_x.py",
            "tests/env/test_x.py",
        ] {
            if is_denied_edit_surface(allowed) {
                return Err(format!("`{allowed}` was denied by a non-prefix match"));
            }
        }
        // Built at runtime so no local absolute path is committed as a literal.
        let drive = char::from(b'Z');
        let drive_forward = format!("{drive}:/x.py");
        for rejected in ["   ", "\t", "c:", drive_forward.as_str()] {
            if check_portable_path(rejected).is_ok() {
                return Err(format!("portable path check accepted `{rejected}`"));
            }
        }
        for accepted in ["tests/test_x.py", "a/b.c/d", "..hidden/x.py", "a/b..c"] {
            check_portable_path(accepted)
                .map_err(|error| format!("`{accepted}` was refused: {error}"))?;
        }
        for (raw, expected) in [
            ("./a//b/./c.py", "a/b/c.py"),
            ("a/b/", "a/b"),
            ("tests/test_x.py", "tests/test_x.py"),
        ] {
            if normalize_repo_relative_path(raw) != expected {
                return Err(format!("`{raw}` did not normalize to `{expected}`"));
            }
        }
        Ok(())
    }

    #[test]
    fn selection_manifest_load_refuses_every_malformed_envelope() -> Result<(), String> {
        let root = TempRoot::new("manifest-load")?;
        let path = root.path().join("manifest.json");

        // Positive control: bytes digest exactly, rows come back verbatim.
        let good = manifest_text_of(vec![sample_row()?])?;
        write_text(&path, &good)?;
        let (digest, rows) = load_selection_manifest(&path)?;
        if digest != sha256_hex(good.as_bytes()) || rows.len() != 1 {
            return Err(format!(
                "control manifest did not load: {digest} {}",
                rows.len()
            ));
        }

        let envelope = |overrides: Value| -> Result<String, String> {
            let mut base = serde_json::json!({
                "schema_version": MANIFEST_SCHEMA_VERSION,
                "kind": MANIFEST_KIND,
                "spec": BINDING_SPEC,
                "selections": [],
            });
            let target = base
                .as_object_mut()
                .ok_or_else(|| "envelope is not an object".to_string())?;
            for (key, value) in overrides.as_object().cloned().unwrap_or_default() {
                if value.is_null() {
                    target.remove(&key);
                } else {
                    target.insert(key, value);
                }
            }
            serde_json::to_string(&base).map_err(|error| error.to_string())
        };
        let mut unknown_row = sample_row()?;
        unknown_row
            .as_object_mut()
            .ok_or_else(|| "sample row is not an object".to_string())?
            .insert("surprise".into(), "x".into());
        let mut no_attempt = sample_row()?;
        no_attempt
            .as_object_mut()
            .ok_or_else(|| "sample row is not an object".to_string())?
            .remove("attempt_id");
        let duplicate_rows = serde_json::json!([sample_row()?, sample_row()?]);
        let cases: Vec<(&str, String, &str)> = vec![
            ("not json", "{".to_string(), "is not well-formed JSON"),
            (
                "duplicate key",
                "{\"kind\":\"a\",\"kind\":\"b\"}".to_string(),
                "duplicate object key `kind`",
            ),
            (
                "array top level",
                "[]".to_string(),
                "subject=`selection manifest`: must be a JSON object",
            ),
            (
                "unknown top key",
                envelope(serde_json::json!({"extra": "x"}))?,
                "unknown field `extra`",
            ),
            (
                "wrong kind",
                envelope(serde_json::json!({"kind": "other_manifest"}))?,
                "field `kind` must be `python_repair_trust_manifest`, got `other_manifest`",
            ),
            (
                "wrong schema",
                envelope(serde_json::json!({"schema_version": "9.9"}))?,
                "field `schema_version` must be `0.1`, got `9.9`",
            ),
            (
                "wrong spec",
                envelope(serde_json::json!({"spec": "RIPR-SPEC-0001"}))?,
                "field `spec` must be `RIPR-SPEC-0176`",
            ),
            (
                "missing selections",
                envelope(serde_json::json!({"selections": null}))?,
                "must contain a selections array",
            ),
            (
                "selections not array",
                envelope(serde_json::json!({"selections": {"a": 1}}))?,
                "must contain a selections array",
            ),
            (
                "row not object",
                envelope(serde_json::json!({"selections": [3]}))?,
                "subject=`selection row`: must be a JSON object",
            ),
            (
                "unknown row key",
                envelope(serde_json::json!({"selections": [unknown_row]}))?,
                "subject=`selection row`: unknown field `surprise`",
            ),
            (
                "row without attempt",
                envelope(serde_json::json!({"selections": [no_attempt]}))?,
                "field=`attempt_id`: required field is missing",
            ),
            (
                "duplicate attempts",
                envelope(serde_json::json!({"selections": duplicate_rows}))?,
                "duplicate attempt identity `att-test-1`",
            ),
        ];
        for (label, text, needle) in cases {
            write_text(&path, &text)?;
            match load_selection_manifest(&path) {
                Err(error) if error.contains(needle) => {}
                other => return Err(format!("case `{label}` expected `{needle}`: {other:?}")),
            }
        }

        // Non-UTF-8 bytes and an absent file are named distinctly.
        std::fs::write(&path, [0xff_u8, 0xfe, 0x00]).map_err(|error| error.to_string())?;
        match load_selection_manifest(&path) {
            Err(error) if error.contains("is not UTF-8") => {}
            other => return Err(format!("non-UTF-8 manifest was not refused: {other:?}")),
        }
        match load_selection_manifest(&root.path().join("absent.json")) {
            Err(error) if error.contains("is not readable") => {}
            other => return Err(format!("an absent manifest was not refused: {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn target_alignment_requires_the_packet_selected_target() -> Result<(), String> {
        let sample = sample_row()?;
        let verified = verify_row_binding(as_object(&sample, "row")?, "att-test-1")?;
        verify_row_target_alignment(&verified, &policy_for("tests/test_pricing.py")?)
            .map_err(|error| format!("the selected target was refused: {error}"))?;
        match verify_row_target_alignment(&verified, &policy_for("tests/test_other.py")?) {
            Err(error)
                if error.contains("wrong target")
                    && error.contains("`tests/test_pricing.py`")
                    && error.contains("`tests/test_other.py`") => {}
            other => {
                return Err(format!(
                    "a similar-but-different target was bound: {other:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn prepare_row_checks_head_inventory_and_target_identity() -> Result<(), String> {
        let root = TempRoot::new("prepare-row")?;
        run_git(root.path(), &["init", "-q"])?;
        let head = "c".repeat(40);
        let sample = sample_row()?;
        let row = as_object(&sample, "row")?;
        let policy = policy_for("tests/test_pricing.py")?;

        // The target is absent from the inventory: a zero-match refusal.
        match verify_row_for_prepare(row, "att-test-1", &head, &policy, root.path()) {
            Err(error) if error.contains("matches no file in the repository inventory") => {}
            other => return Err(format!("a missing target was bound: {other:?}")),
        }

        // An untracked file counts as inventory: the control now binds.
        write_text(
            &root.path().join("tests/test_pricing.py"),
            "def test_x(): pass\n",
        )?;
        let bound = verify_row_for_prepare(row, "att-test-1", &head, &policy, root.path())
            .map_err(|error| format!("the control row was refused: {error}"))?;
        if bound.target_path != "tests/test_pricing.py" {
            return Err(format!("control bound `{}`", bound.target_path));
        }

        // A selection made at another commit is stale before any attempt exists.
        let other_head = "d".repeat(40);
        match verify_row_for_prepare(row, "att-test-1", &other_head, &policy, root.path()) {
            Err(error)
                if error.contains("stale selection")
                    && error.contains(&format!("pins head `{head}`"))
                    && error.contains(&format!("HEAD is `{other_head}`")) => {}
            other => return Err(format!("a stale head was bound: {other:?}")),
        }

        // The packet's selected target must be the row's target.
        let other_policy = policy_for("tests/test_other.py")?;
        match verify_row_for_prepare(row, "att-test-1", &head, &other_policy, root.path()) {
            Err(error) if error.contains("wrong target") => {}
            other => return Err(format!("a mismatched packet target was bound: {other:?}")),
        }

        // The shared binding checks run first: a substituted attempt fails.
        match verify_row_for_prepare(row, "att-other", &head, &policy, root.path()) {
            Err(error) if error.contains("does not match the requested attempt") => {}
            other => return Err(format!("a substituted attempt was bound: {other:?}")),
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prepare_row_refuses_a_case_ambiguous_target() -> Result<(), String> {
        let root = TempRoot::new("prepare-ambiguous")?;
        run_git(root.path(), &["init", "-q"])?;
        write_text(&root.path().join("tests/test_pricing.py"), "a\n")?;
        let sample = sample_row()?;
        let row = as_object(&sample, "row")?;
        let policy = policy_for("tests/test_pricing.py")?;
        let head = "c".repeat(40);
        verify_row_for_prepare(row, "att-test-1", &head, &policy, root.path())
            .map_err(|error| format!("the unambiguous control was refused: {error}"))?;

        write_text(&root.path().join("tests/Test_Pricing.py"), "b\n")?;
        let spellings = std::fs::read_dir(root.path().join("tests"))
            .map_err(|error| error.to_string())?
            .count();
        if spellings != 2 {
            return Err(format!("fixture did not create two spellings: {spellings}"));
        }
        match verify_row_for_prepare(row, "att-test-1", &head, &policy, root.path()) {
            Err(error)
                if error.contains("matches 2 candidate files") && error.contains("ambiguous") => {}
            other => return Err(format!("an ambiguous target was bound: {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn repository_inventory_failure_is_reported_not_swallowed() -> Result<(), String> {
        let root = TempRoot::new("inventory")?;
        run_git(root.path(), &["init", "-q"])?;
        let listed = git_inventory(root.path(), &["ls-files", "-z"])?;
        if !listed.is_empty() {
            return Err(format!("an empty repository listed files: {listed:?}"));
        }
        match git_inventory(root.path(), &["no-such-git-subcommand"]) {
            Err(error) if error.contains("repository inventory failed") => {}
            other => return Err(format!("a failing git call was swallowed: {other:?}")),
        }
        Ok(())
    }

    fn tampered_record(mutate: Mutation) -> Result<RetainedBinding, String> {
        let mut record = retained_record_fixture();
        let object = record
            .as_object_mut()
            .ok_or_else(|| "retained fixture is not an object".to_string())?;
        mutate(object);
        Ok(RetainedBinding {
            artifact_sha256: String::new(),
            value: record,
        })
    }

    fn reverify_error(retained: &RetainedBinding) -> Result<String, String> {
        match reverify_for_apply(
            "seam-under-test",
            &policy_for("tests/test_pricing.py")?,
            b"packet bytes",
            retained,
            &granted(),
        ) {
            Err(error) => Ok(error),
            Ok(_) => Err("a tampered retained record passed re-verification".to_string()),
        }
    }

    fn edit_block(record: &mut Map<String, Value>, block: &str, field: &str, value: Value) {
        if let Some(object) = record.get_mut(block).and_then(Value::as_object_mut) {
            object.insert(field.to_string(), value);
        }
    }

    #[test]
    fn reverify_refuses_each_tampered_record_envelope_and_authorization() -> Result<(), String> {
        // Positive control reaches the manifest stage, so every refusal below
        // is the targeted structural check rather than a missing manifest.
        let control = reverify_error(&tampered_record(|_| {})?)?;
        if !control.contains("selection manifest") {
            return Err(format!(
                "control did not reach the manifest stage: {control}"
            ));
        }

        let cases: Vec<(&str, Mutation, &str)> = vec![
            (
                "schema_version",
                |record| {
                    record.insert("schema_version".into(), "0.2".into());
                },
                "field `schema_version` must be `0.1` for the apply phase",
            ),
            (
                "kind",
                |record| {
                    record.insert("kind".into(), "other_binding".into());
                },
                "field `kind` must be `python_repair_driver_binding`",
            ),
            (
                "spec",
                |record| {
                    record.insert("spec".into(), "RIPR-SPEC-0001".into());
                },
                "field `spec` must be `RIPR-SPEC-0176`",
            ),
            (
                "phase",
                |record| {
                    record.insert("phase".into(), "apply".into());
                },
                "field `phase` must be `prepare` for the apply phase, got `apply`",
            ),
            (
                "replayed apply block",
                |record| {
                    record.insert("apply".into(), serde_json::json!({}));
                },
                // The closed prepare-key allowlist refuses the block before the
                // explicit replay guard is reached, so a replayed apply record
                // still fails closed.
                "unknown field `apply`",
            ),
            (
                "seam",
                |record| {
                    record.insert("seam_id".into(), "other-seam".into());
                },
                "names seam `other-seam` but the selected attempt belongs to seam `seam-under-test`",
            ),
            (
                "unknown top-level key",
                |record| {
                    record.insert("surprise".into(), "x".into());
                },
                "subject=`retained binding record`: unknown field `surprise`",
            ),
            (
                "missing authorization",
                |record| {
                    record.remove("authorization");
                },
                "retained binding record is missing authorization",
            ),
            (
                "authorization not an object",
                |record| {
                    record.insert("authorization".into(), serde_json::json!("granted"));
                },
                "subject=`retained authorization`: must be a JSON object",
            ),
            (
                "unknown authorization key",
                |record| edit_block(record, "authorization", "extra", "x".into()),
                "subject=`retained authorization`: unknown field `extra`",
            ),
            (
                "authorization status",
                |record| edit_block(record, "authorization", "status", "denied".into()),
                "retained binding authorization status is `denied`",
            ),
            (
                "different authority",
                |record| edit_block(record, "authorization", "authority", "someone-else".into()),
                "does not match the retained authorization authority",
            ),
            (
                "authorization method",
                |record| edit_block(record, "authorization", "method", "ambient-config".into()),
                "`ambient-config` is not the explicit-operator-flags authority",
            ),
            (
                "missing driver block",
                |record| {
                    record.remove("driver");
                },
                "retained binding record is missing driver",
            ),
            (
                "unknown nested key",
                |record| edit_block(record, "config", "extra", "x".into()),
                "subject=`retained binding config`: unknown field `extra`",
            ),
            (
                "before snapshot digest",
                |record| edit_block(record, "input", "before_snapshot_sha256", "XYZ".into()),
                "field=`before_snapshot_sha256`: must be bare lowercase sha256 hex",
            ),
            (
                "forbidden not an array",
                |record| edit_block(record, "edit_surface", "forbidden", "target/".into()),
                "field=`forbidden`: must be an array of paths",
            ),
            (
                "allowed entry not a string",
                |record| edit_block(record, "edit_surface", "allowed", serde_json::json!([1])),
                "field=`allowed[0]`: path must be a string",
            ),
            (
                "absolute allowed path",
                |record| {
                    edit_block(
                        record,
                        "edit_surface",
                        "allowed",
                        serde_json::json!(["/etc/test_x.py"]),
                    );
                },
                "must be repo-relative, not absolute",
            ),
            (
                "missing non_claims",
                |record| {
                    record.remove("non_claims");
                },
                "must carry its non_claims",
            ),
            (
                "dropped non-claim",
                |record| {
                    record.insert(
                        "non_claims".into(),
                        serde_json::json!([BINDING_NON_CLAIMS[0], BINDING_NON_CLAIMS[1]]),
                    );
                },
                "exactly the standing non-claims",
            ),
            (
                "smuggled non-claim",
                |record| {
                    record.insert(
                        "non_claims".into(),
                        serde_json::json!([
                            BINDING_NON_CLAIMS[0],
                            BINDING_NON_CLAIMS[1],
                            BINDING_NON_CLAIMS[2],
                            "verification passed"
                        ]),
                    );
                },
                "exactly the standing non-claims",
            ),
            (
                "non-string non-claim",
                |record| {
                    record.insert("non_claims".into(), serde_json::json!([7]));
                },
                "non_claims[0]: non-claim must be a string",
            ),
            (
                "missing trust",
                |record| {
                    record.remove("trust");
                },
                "retained binding record is missing trust",
            ),
            (
                "unknown trust key",
                |record| edit_block(record, "trust", "extra", "x".into()),
                "subject=`retained binding trust`: unknown field `extra`",
            ),
            (
                "missing manifest path",
                |record| {
                    record.remove(TELEMETRY_MANIFEST_PATH_FIELD);
                },
                "field=`selection_manifest_path`: required field is missing",
            ),
        ];
        for (label, mutate, needle) in cases {
            let error = reverify_error(&tampered_record(mutate)?)?;
            if !error.contains(needle) {
                return Err(format!("case `{label}` expected `{needle}`: {error}"));
            }
            if error.contains("selection manifest") && !needle.contains("manifest") {
                return Err(format!(
                    "case `{label}` leaked to the manifest stage: {error}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn reverify_requires_a_fresh_matching_authorization() -> Result<(), String> {
        let retained = tampered_record(|_| {})?;
        let policy = policy_for("tests/test_pricing.py")?;
        // Surrounding whitespace is trimmed before the authority comparison, so
        // the same identity still reaches the manifest stage.
        let padded = EditAuthorization {
            authorized: true,
            authority: Some(format!("  {AUTHORITY_IDENTITY}  ")),
        };
        match reverify_for_apply(
            "seam-under-test",
            &policy,
            b"packet bytes",
            &retained,
            &padded,
        ) {
            Err(error) if error.contains("selection manifest") => {}
            other => return Err(format!("the same trimmed authority was refused: {other:?}")),
        }
        let refusals = [
            (
                EditAuthorization {
                    authorized: false,
                    authority: Some(AUTHORITY_IDENTITY.to_string()),
                },
                "refuses to record the applied edit: explicit --edit-authorized",
            ),
            (
                EditAuthorization {
                    authorized: true,
                    authority: Some("Test-Operator".to_string()),
                },
                "does not match the retained authorization authority",
            ),
        ];
        for (authorization, needle) in refusals {
            match reverify_for_apply(
                "seam-under-test",
                &policy,
                b"packet bytes",
                &retained,
                &authorization,
            ) {
                Err(error) if error.contains(needle) => {}
                other => return Err(format!("expected `{needle}`: {other:?}")),
            }
        }
        Ok(())
    }

    fn retained_for_manifest(
        manifest_path: &Path,
        manifest_sha256: &str,
        attempt_id: &str,
    ) -> Result<RetainedBinding, String> {
        let mut record = retained_record_fixture();
        let object = record
            .as_object_mut()
            .ok_or_else(|| "retained fixture is not an object".to_string())?;
        object.insert(
            TELEMETRY_MANIFEST_PATH_FIELD.to_string(),
            Value::String(manifest_path.display().to_string()),
        );
        let trust = object
            .get_mut("trust")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "retained fixture is missing trust".to_string())?;
        trust.insert("attempt_id".into(), attempt_id.into());
        trust.insert("selection_manifest_sha256".into(), manifest_sha256.into());
        Ok(RetainedBinding {
            artifact_sha256: String::new(),
            value: record,
        })
    }

    #[test]
    fn reverify_binds_the_manifest_row_target_and_packet_digests() -> Result<(), String> {
        let root = TempRoot::new("reverify-manifest")?;
        let path = root.path().join("trust-manifest.json");
        let text = manifest_text_of(vec![sample_row()?])?;
        write_text(&path, &text)?;
        let pinned = sha256_hex(text.as_bytes());
        let policy = policy_for("tests/test_pricing.py")?;
        let retained = retained_for_manifest(&path, &pinned, "att-test-1")?;
        let reverify = |retained: &RetainedBinding, policy: &EditCagePolicy, packet: &[u8]| {
            reverify_for_apply("seam-under-test", policy, packet, retained, &granted())
        };

        // Positive control: every anchor intact returns the verified row.
        let verified = reverify(&retained, &policy, b"packet bytes")
            .map_err(|error| format!("the intact retained binding was refused: {error}"))?;
        if verified.attempt_id != "att-test-1"
            || verified.selection_manifest_sha256 != pinned
            || verified.selection_manifest_path != path.display().to_string()
            || verified.target_path != "tests/test_pricing.py"
        {
            return Err(format!(
                "control verification lost an identity: {verified:?}"
            ));
        }

        // A changed packet is refused by digest.
        match reverify(&retained, &policy, b"a different packet") {
            Err(error) if error.contains("stale packet: the retained binding pins packet") => {}
            other => return Err(format!("a changed packet was accepted: {other:?}")),
        }

        // A packet whose selected target moved is a wrong-target refusal.
        match reverify(
            &retained,
            &policy_for("tests/test_other.py")?,
            b"packet bytes",
        ) {
            Err(error)
                if error.contains("stale packet rejected before recording the applied edit")
                    && error.contains("wrong target") => {}
            other => return Err(format!("a moved packet target was accepted: {other:?}")),
        }

        // A replaced manifest no longer digests to the pin.
        let replaced_text = format!("{text}\n");
        write_text(&path, &replaced_text)?;
        match reverify(&retained, &policy, b"packet bytes") {
            Err(error)
                if error.contains("stale selection manifest")
                    && error.contains(&sha256_hex(replaced_text.as_bytes())) => {}
            other => return Err(format!("a replaced manifest was accepted: {other:?}")),
        }

        // A row removed from a re-pinned manifest cannot be replaced by name.
        let emptied = manifest_text_of(Vec::new())?;
        write_text(&path, &emptied)?;
        let repinned = retained_for_manifest(&path, &sha256_hex(emptied.as_bytes()), "att-test-1")?;
        match reverify(&repinned, &policy, b"packet bytes") {
            Err(error) if error.contains("no longer exists in the accepted selection manifest") => {
            }
            other => return Err(format!("a deleted row was accepted: {other:?}")),
        }

        // A row edited under a re-pinned manifest fails its own digest anchor.
        let mut edited = sample_row()?;
        edited
            .as_object_mut()
            .ok_or_else(|| "sample row is not an object".to_string())?
            .insert("oracle".into(), "assert nothing".into());
        let edited_text = manifest_text_of(vec![edited])?;
        write_text(&path, &edited_text)?;
        let repinned =
            retained_for_manifest(&path, &sha256_hex(edited_text.as_bytes()), "att-test-1")?;
        match reverify(&repinned, &policy, b"packet bytes") {
            Err(error)
                if error.contains("stale packet rejected before recording the applied edit")
                    && error.contains("digest mismatch") => {}
            other => return Err(format!("an edited row was accepted: {other:?}")),
        }

        // A missing manifest is an unreadable-manifest refusal, not a pass.
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        match reverify(&retained, &policy, b"packet bytes") {
            Err(error) if error.contains("is not readable") => {}
            other => return Err(format!("a missing manifest was accepted: {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn manifest_confirmation_separates_operational_failures_from_replacement() -> Result<(), String>
    {
        let root = TempRoot::new("confirm-operational")?;
        let path = root.path().join("manifest.json");
        let text = manifest_text_of(Vec::new())?;
        write_text(&path, &text)?;
        let pinned = sha256_hex(text.as_bytes());
        let retained_with = |value: Value| RetainedBinding {
            artifact_sha256: String::new(),
            value,
        };
        let path_text = path.display().to_string();

        // Positive control for the fixture shape used by the refusals below.
        confirm_manifest_unchanged(&retained_with(serde_json::json!({
            "trust": {"selection_manifest_sha256": pinned},
            "selection_manifest_path": path_text,
        })))
        .map_err(|error| format!("the intact control was refused: {error}"))?;

        let gone = root.path().join("gone.json").display().to_string();
        let cases = [
            (
                "not an object",
                serde_json::json!([]),
                "subject=`retained binding record`: must be a JSON object",
            ),
            (
                "missing trust",
                serde_json::json!({"selection_manifest_path": path_text}),
                "retained binding record is missing trust",
            ),
            (
                "missing pin",
                serde_json::json!({"trust": {}, "selection_manifest_path": path_text}),
                "field=`selection_manifest_sha256`: required field is missing",
            ),
            (
                "missing path",
                serde_json::json!({"trust": {"selection_manifest_sha256": pinned}}),
                "field=`selection_manifest_path`: required field is missing",
            ),
            (
                "unreadable manifest",
                serde_json::json!({
                    "trust": {"selection_manifest_sha256": pinned},
                    "selection_manifest_path": gone,
                }),
                "stale packet rejected before the durable finish",
            ),
        ];
        for (label, value, needle) in cases {
            match confirm_manifest_unchanged(&retained_with(value)) {
                Err(ManifestConfirmationError::Operational(message))
                    if message.contains(needle) => {}
                Err(other) => {
                    return Err(format!(
                        "case `{label}` expected operational `{needle}`: {other}"
                    ));
                }
                Ok(()) => return Err(format!("case `{label}` passed the confirmation")),
            }
        }

        // The two variants render the same message through both conversions.
        for error in [
            ManifestConfirmationError::Replaced("replaced".to_string()),
            ManifestConfirmationError::Operational("operational".to_string()),
        ] {
            let message = error.message().to_string();
            if error.to_string() != message || String::from(error) != message {
                return Err(format!("message conversions disagreed for `{message}`"));
            }
        }
        Ok(())
    }

    #[test]
    fn config_profile_reports_a_present_ripr_toml() -> Result<(), String> {
        let root = TempRoot::new("config-profile")?;
        if detect_config_profile(root.path()) != "default" {
            return Err("an absent ripr.toml must be the default profile".to_string());
        }
        write_text(&root.path().join("ripr.toml"), "")?;
        if detect_config_profile(root.path()) != "subject-ripr-toml" {
            return Err("a present ripr.toml must be the subject profile".to_string());
        }
        Ok(())
    }

    #[test]
    fn render_record_carries_optional_identities_only_when_present() -> Result<(), String> {
        let root = TempRoot::new("render-optional")?;
        let policy = policy_for("tests/test_pricing.py")?;
        let sample = sample_row()?;
        let mut verified = verify_row_binding(as_object(&sample, "row")?, "att-test-1")?;
        let digest = sha256_hex(b"x");
        let render = |verified: &VerifiedSelection,
                      durable: Option<&str>,
                      artifact: Option<&str>,
                      apply: Option<Value>| {
            render_record(
                root.path(),
                RecordIdentity {
                    seam_id: "seam-under-test",
                    repository_head: &verified.head,
                    phase: if apply.is_some() { "apply" } else { "prepare" },
                    durable_attempt_id: durable,
                    binding_artifact_sha256: artifact,
                    verified,
                    authority: AUTHORITY_IDENTITY,
                    packet_sha256: &digest,
                    before_snapshot_sha256: &digest,
                    policy: &policy,
                },
                apply,
            )
        };

        // Minimal prepare record: no optional trust fields, no apply members.
        let bare = render(&verified, None, None, None)?;
        for pointer in [
            "/trust/tree",
            "/trust/source_currentness",
            "/trust/limitation",
            "/durable_attempt_id",
            "/binding_artifact_sha256",
            "/apply",
        ] {
            if bare.pointer(pointer).is_some() {
                return Err(format!("the bare record invented `{pointer}`"));
            }
        }
        if bare.pointer("/config/profile").and_then(Value::as_str) != Some("default")
            || bare
                .pointer("/authorization/status")
                .and_then(Value::as_str)
                != Some("granted")
            || bare
                .pointer("/edit_surface/allowed/0")
                .and_then(Value::as_str)
                != Some("tests/test_pricing.py")
        {
            return Err(format!("the bare record lost a standing field: {bare}"));
        }
        // The rendered prepare record is accepted by the prepare-key allowlist.
        let keys = bare
            .as_object()
            .ok_or_else(|| "record is not an object".to_string())?;
        reject_unknown_keys(keys, &PREPARE_RECORD_KEYS, "rendered record")?;

        verified.tree = Some(sha256_hex(b"tree"));
        verified.source_currentness = Some("current".to_string());
        verified.limitation = Some("static evidence only".to_string());
        let full = render(
            &verified,
            Some("attempt-1"),
            Some("artifactdigest"),
            Some(serde_json::json!({"patch_sha256": "p"})),
        )?;
        for (pointer, expected) in [
            ("/trust/tree", sha256_hex(b"tree")),
            ("/trust/source_currentness", "current".to_string()),
            ("/trust/limitation", "static evidence only".to_string()),
            ("/durable_attempt_id", "attempt-1".to_string()),
            ("/binding_artifact_sha256", "artifactdigest".to_string()),
            ("/apply/patch_sha256", "p".to_string()),
            ("/phase", "apply".to_string()),
        ] {
            if full.pointer(pointer).and_then(Value::as_str) != Some(expected.as_str()) {
                return Err(format!("the full record lost `{pointer}`: {full}"));
            }
        }
        Ok(())
    }

    #[test]
    fn write_record_file_publishes_pretty_json_and_reports_failure() -> Result<(), String> {
        let root = TempRoot::new("write-record")?;
        let record = serde_json::json!({"kind": BINDING_KIND, "phase": "prepare"});
        let path = root.path().join("record.json");
        write_record_file(&path, &record)?;
        let written = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        if !written.ends_with("}\n") || !written.contains("\n  \"kind\"") {
            return Err(format!(
                "record was not pretty JSON with a newline: {written:?}"
            ));
        }
        if parse_strict_json(&written)? != record {
            return Err("the written record did not round trip".to_string());
        }
        // A missing parent directory is a reported failure, with no residue.
        let orphan = root.path().join("absent-dir/record.json");
        if write_record_file(&orphan, &record).is_ok() {
            return Err("a write into a missing directory reported success".to_string());
        }
        if orphan.exists() {
            return Err("a failed write left a record behind".to_string());
        }
        Ok(())
    }

    /// A one-commit repository holding the selected test file, plus the
    /// accepted manifest and a before snapshot, ready for `prepare_binding`.
    struct PrepareFixture {
        root: TempRoot,
        head: String,
        row: Value,
    }

    fn prepare_fixture(tag: &str) -> Result<PrepareFixture, String> {
        let root = TempRoot::new(tag)?;
        run_git(root.path(), &["init", "-q"])?;
        write_text(
            &root.path().join("tests/test_pricing.py"),
            "def test_x(): pass\n",
        )?;
        run_git(root.path(), &["add", "."])?;
        run_git(root.path(), &["config", "user.email", "test@example.com"])?;
        run_git(root.path(), &["config", "user.name", "Test"])?;
        run_git(root.path(), &["commit", "-q", "-m", "base"])?;
        let head = crate::agent::artifact::current_git_head(root.path())?;
        let pinned = head.clone();
        let row = redigested_row(|row| {
            row.insert("head".into(), pinned.into());
        })?;
        write_text(
            &root.path().join("trust-manifest.json"),
            &manifest_text_of(vec![row.clone()])?,
        )?;
        write_text(&root.path().join("before.json"), "{\"before\":true}\n")?;
        Ok(PrepareFixture { root, head, row })
    }

    fn prepare_with(
        fixture: &PrepareFixture,
        manifest: &str,
        attempt: &str,
        authorization: &EditAuthorization,
        policy: &EditCagePolicy,
        before: &str,
    ) -> Result<PreparedBinding, String> {
        prepare_binding(
            fixture.root.path(),
            "seam-under-test",
            policy,
            &fixture.root.path().join(before),
            b"packet bytes",
            &PythonRepairTrustSelection {
                manifest_path: PathBuf::from(manifest),
                attempt_id: attempt.to_string(),
            },
            authorization,
        )
    }

    #[test]
    fn prepare_binding_refuses_before_writing_any_record_and_then_round_trips() -> Result<(), String>
    {
        let fixture = prepare_fixture("prepare-e2e")?;
        let root = fixture.root.path();
        let policy = policy_for("tests/test_pricing.py")?;
        let stale_policy = policy_for("tests/test_other.py")?;
        let record_path = root.join(PREPARE_RECORD_COMPAT_PATH);
        let unauthorized = EditAuthorization {
            authorized: false,
            authority: Some(AUTHORITY_IDENTITY.to_string()),
        };
        let ok = "trust-manifest.json";

        // A manifest outside the repository cannot govern it.
        let outside = TempRoot::new("prepare-outside")?;
        let outside_manifest = outside.path().join("trust-manifest.json");
        write_text(
            &outside_manifest,
            &manifest_text_of(vec![fixture.row.clone()])?,
        )?;
        let outside_text = outside_manifest.display().to_string();

        let cases: Vec<(&str, Result<PreparedBinding, String>, &str)> = vec![
            (
                "unauthorized",
                prepare_with(
                    &fixture,
                    ok,
                    "att-test-1",
                    &unauthorized,
                    &policy,
                    "before.json",
                ),
                "refuses to prepare the edit transaction",
            ),
            (
                "manifest absent",
                prepare_with(
                    &fixture,
                    "absent.json",
                    "att-test-1",
                    &granted(),
                    &policy,
                    "before.json",
                ),
                "is not readable",
            ),
            (
                "manifest outside root",
                prepare_with(
                    &fixture,
                    &outside_text,
                    "att-test-1",
                    &granted(),
                    &policy,
                    "before.json",
                ),
                "is outside the repository root",
            ),
            (
                "attempt outside denominator",
                prepare_with(
                    &fixture,
                    ok,
                    "att-ghost",
                    &granted(),
                    &policy,
                    "before.json",
                ),
                "outside the accepted selection denominator (1 rows present)",
            ),
            (
                "packet target moved",
                prepare_with(
                    &fixture,
                    ok,
                    "att-test-1",
                    &granted(),
                    &stale_policy,
                    "before.json",
                ),
                "wrong target",
            ),
            (
                "before snapshot absent",
                prepare_with(
                    &fixture,
                    ok,
                    "att-test-1",
                    &granted(),
                    &policy,
                    "no-before.json",
                ),
                "requires the before snapshot",
            ),
        ];
        for (label, result, needle) in cases {
            match result {
                Err(error) if error.contains(needle) => {}
                other => return Err(format!("case `{label}` expected `{needle}`: {other:?}")),
            }
            if record_path.exists() {
                return Err(format!("case `{label}` wrote a record despite refusing"));
            }
        }

        // A manifest whose row pins another commit is stale before editing.
        let moved_row = redigested_row(|row| {
            row.insert("head".into(), "e".repeat(40).into());
        })?;
        write_text(&root.join(ok), &manifest_text_of(vec![moved_row])?)?;
        match prepare_with(
            &fixture,
            ok,
            "att-test-1",
            &granted(),
            &policy,
            "before.json",
        ) {
            Err(error) if error.contains("stale selection") => {}
            other => return Err(format!("a stale head was prepared: {other:?}")),
        }
        if record_path.exists() {
            return Err("a stale selection wrote a record".to_string());
        }

        // Positive control: the accepted manifest prepares, root-relative.
        let manifest_text = manifest_text_of(vec![fixture.row.clone()])?;
        write_text(&root.join(ok), &manifest_text)?;
        std::fs::create_dir_all(root.join("target/ripr/workflow"))
            .map_err(|error| error.to_string())?;
        let prepared = prepare_with(
            &fixture,
            ok,
            "att-test-1",
            &granted(),
            &policy,
            "before.json",
        )
        .map_err(|error| format!("the accepted selection was refused: {error}"))?;
        if prepared.record_path != record_path || !record_path.exists() {
            return Err("the prepare record was not published at its compat path".to_string());
        }
        let manifest_sha = sha256_hex(manifest_text.as_bytes());
        let packet_sha = sha256_hex(b"packet bytes");
        let before_sha = sha256_hex(b"{\"before\":true}\n");
        let pointer = |path: &str| prepared.record.pointer(path).and_then(Value::as_str);
        if prepared.verified.selection_manifest_sha256 != manifest_sha
            || pointer("/trust/selection_manifest_sha256") != Some(manifest_sha.as_str())
            || pointer("/repository_head") != Some(fixture.head.as_str())
            || pointer("/input/packet_sha256") != Some(packet_sha.as_str())
            || pointer("/input/before_snapshot_sha256") != Some(before_sha.as_str())
            || pointer("/phase") != Some("prepare")
        {
            return Err(format!(
                "the prepare record lost a bound identity: {}",
                prepared.record
            ));
        }
        let on_disk = std::fs::read_to_string(&record_path).map_err(|error| error.to_string())?;
        if parse_strict_json(&on_disk)? != prepared.record {
            return Err("the published record differs from the returned record".to_string());
        }

        // The producer's own record is what the apply phase accepts, and the
        // apply phase then refuses a changed packet or a replaced manifest.
        let retained = RetainedBinding {
            artifact_sha256: String::new(),
            value: prepared.record.clone(),
        };
        let verified = reverify_for_apply(
            "seam-under-test",
            &policy,
            b"packet bytes",
            &retained,
            &granted(),
        )
        .map_err(|error| format!("the prepared record failed re-verification: {error}"))?;
        if verified.selection_digest != prepared.verified.selection_digest {
            return Err("re-verification rebound a different row digest".to_string());
        }
        match reverify_for_apply(
            "seam-under-test",
            &policy,
            b"other packet",
            &retained,
            &granted(),
        ) {
            Err(error) if error.contains("stale packet") => {}
            other => return Err(format!("a changed packet re-verified: {other:?}")),
        }
        confirm_manifest_unchanged(&retained)
            .map_err(|error| format!("the untouched manifest failed confirmation: {error}"))?;
        write_text(&root.join(ok), &format!("{manifest_text}\n"))?;
        match confirm_manifest_unchanged(&retained) {
            Err(ManifestConfirmationError::Replaced(_)) => {}
            Err(ManifestConfirmationError::Operational(message)) => {
                return Err(format!("replacement read as operational: {message}"));
            }
            Ok(()) => return Err("a replaced manifest was confirmed".to_string()),
        }
        Ok(())
    }

    #[test]
    fn retained_binding_load_refuses_an_unknown_attempt() -> Result<(), String> {
        let root = TempRoot::new("retained-load")?;
        let attempt = crate::app::repair_attempt::RepairAttemptId::parse(format!(
            "repair-attempt-{}",
            "0".repeat(24)
        ))?;
        match load_retained_binding_from(root.path(), None, &attempt) {
            Err(_) => Ok(()),
            Ok(found) => Err(format!("an unknown attempt produced a binding: {found:?}")),
        }
    }
}
