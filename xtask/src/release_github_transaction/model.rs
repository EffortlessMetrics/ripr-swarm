//! Typed request for one GitHub tag/release publication transaction (ripr#1644).
//!
//! The request is immutable input. Nothing here reads or writes GitHub.

use crate::blind_journey::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) const REQUEST_SCHEMA: &str = "ripr.release.github_transaction.request.v1";

/// Subject classes a GitHub authorization packet may carry (ripr#1651). A
/// subject `kind` is one of these and must equal the packet subject's class.
const GITHUB_SUBJECT_KINDS: [&str; 5] = [
    "server_archive",
    "server_manifest",
    "checksums",
    "vsix",
    "attestation_bundle",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    PublicRc,
    Stable,
}

/// Operation names shared with the release authorization packet (ripr#1651).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Operation {
    CreateAnnotatedTag,
    GithubPrereleaseCreate,
    GithubReleaseCreate,
    GithubAssetUpload,
}

impl Operation {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::CreateAnnotatedTag => "create_annotated_tag",
            Self::GithubPrereleaseCreate => "github_prerelease_create",
            Self::GithubReleaseCreate => "github_release_create",
            Self::GithubAssetUpload => "github_asset_upload",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExpectedSubject {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) size: u64,
    pub(crate) sha256: String,
}

/// Channels this transaction can never authorize. Each must be `false`; the
/// fields exist so a request that tries to carry one is rejected, not ignored.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WithheldChannels {
    pub(crate) crates_io: bool,
    pub(crate) npm: bool,
    pub(crate) vs_marketplace: bool,
    pub(crate) open_vsx: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GitHubReleaseTransactionRequest {
    pub(crate) schema: String,
    pub(crate) repository: String,
    pub(crate) mode: Mode,
    pub(crate) product_version: String,
    pub(crate) candidate_sha: String,
    pub(crate) candidate_tree: String,
    /// The exact authorized tag. It is never derived from `product_version`.
    pub(crate) tag: String,
    pub(crate) title: String,
    pub(crate) body_sha256: String,
    pub(crate) prerelease: bool,
    pub(crate) ship_packet_sha256: String,
    /// Required as a key: `null` for a public RC. An omitted key is refused
    /// rather than read as `null`.
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub(crate) stable_decision_sha256: Option<String>,
    pub(crate) subject_inventory_sha256: String,
    pub(crate) subjects: Vec<ExpectedSubject>,
    pub(crate) withheld_channels: WithheldChannels,
}

pub(crate) fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_stable_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let numeric = |part: Option<&str>| {
        part.is_some_and(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part == "0" || !part.starts_with('0'))
        })
    };
    numeric(parts.next())
        && numeric(parts.next())
        && numeric(parts.next())
        && parts.next().is_none()
}

/// Asset names compare case-insensitively: two names that differ only in case
/// are one public name to a reader.
pub(crate) fn normalized_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn valid_subject_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
}

/// SHA-256 over the canonical JSON of the ordered subject list.
pub(crate) fn subject_inventory_sha256(subjects: &[ExpectedSubject]) -> Result<String, String> {
    serde_json::to_string(subjects)
        .map(|json| sha256_hex(json.as_bytes()))
        .map_err(|error| format!("subject inventory serialization failed: {error}"))
}

impl GitHubReleaseTransactionRequest {
    /// The release-creating operation this mode needs authorized.
    pub(crate) fn release_operation(&self) -> Operation {
        match self.mode {
            Mode::PublicRc => Operation::GithubPrereleaseCreate,
            Mode::Stable => Operation::GithubReleaseCreate,
        }
    }

    /// Identity of this exact request: SHA-256 over its canonical JSON, as
    /// bare lowercase hex. The authorization packet names this value, so the
    /// request deliberately does not name the authorization back.
    pub(crate) fn request_id(&self) -> Result<String, String> {
        serde_json::to_string(self)
            .map(|json| sha256_hex(json.as_bytes()))
            .map_err(|error| format!("request serialization failed: {error}"))
    }

    /// Every reason this request cannot be admitted. Empty means admissible.
    pub(crate) fn violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        let mut refuse = |reason: &str| violations.push(reason.to_string());
        if self.schema != REQUEST_SCHEMA {
            refuse("unsupported request schema");
        }
        let repository_ok = self
            .repository
            .split_once('/')
            .is_some_and(|(owner, name)| {
                let part = |value: &str| {
                    !value.is_empty()
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
                };
                part(owner) && part(name)
            });
        if !repository_ok {
            refuse("repository must be owner/name");
        }
        if !is_stable_version(&self.product_version) {
            refuse("product_version must be MAJOR.MINOR.PATCH");
        }
        if !is_hex(&self.candidate_sha, 40) || !is_hex(&self.candidate_tree, 40) {
            refuse("candidate_sha and candidate_tree must be 40 lowercase hex");
        }
        for (name, value) in [
            ("body_sha256", &self.body_sha256),
            ("ship_packet_sha256", &self.ship_packet_sha256),
            ("subject_inventory_sha256", &self.subject_inventory_sha256),
        ] {
            if !is_hex(value, 64) {
                violations.push(format!("{name} must be 64 lowercase hex"));
            }
        }
        if self.title.trim().is_empty() || self.title.contains(['\n', '\r']) {
            violations.push("title must be one non-empty line".to_string());
        }
        match self.mode {
            Mode::PublicRc => {
                let ordinal = self
                    .tag
                    .strip_prefix(&format!("v{}-rc.", self.product_version));
                let ordinal_ok = ordinal.is_some_and(|n| {
                    !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && !n.starts_with('0')
                });
                if !ordinal_ok {
                    violations
                        .push("public_rc tag must be v<product_version>-rc.<ordinal>".to_string());
                }
                if !self.prerelease {
                    violations.push("public_rc requires prerelease=true".to_string());
                }
                if self.stable_decision_sha256.is_some() {
                    violations.push("public_rc must not carry a stable decision".to_string());
                }
            }
            Mode::Stable => {
                if self.tag != format!("v{}", self.product_version) {
                    violations.push("stable tag must be v<product_version>".to_string());
                }
                if self.prerelease {
                    violations.push("stable requires prerelease=false".to_string());
                }
                if !self
                    .stable_decision_sha256
                    .as_deref()
                    .is_some_and(|value| is_hex(value, 64))
                {
                    violations.push("stable requires a 64-hex stable_decision_sha256".to_string());
                }
            }
        }
        let withheld = &self.withheld_channels;
        if withheld.crates_io || withheld.npm || withheld.vs_marketplace || withheld.open_vsx {
            violations.push("a GitHub transaction cannot authorize another channel".to_string());
        }
        if self.subjects.is_empty() {
            violations.push("subjects must not be empty".to_string());
        }
        let mut seen = BTreeSet::new();
        for subject in &self.subjects {
            if !valid_subject_name(&subject.name) {
                violations.push(format!(
                    "subject name `{}` is not a plain file name",
                    subject.name
                ));
            }
            if !seen.insert(normalized_name(&subject.name)) {
                violations.push(format!("subject name `{}` is duplicated", subject.name));
            }
            if !GITHUB_SUBJECT_KINDS.contains(&subject.kind.as_str()) {
                violations.push(format!(
                    "subject `{}` kind `{}` is not a GitHub asset class",
                    subject.name, subject.kind
                ));
            }
            if subject.size == 0 || !is_hex(&subject.sha256, 64) {
                violations.push(format!(
                    "subject `{}` needs a nonzero size and 64-hex sha256",
                    subject.name
                ));
            }
        }
        if !self.subjects.is_sorted_by(|a, b| a.name < b.name) {
            violations.push("subjects must be in strict name order".to_string());
        }
        match subject_inventory_sha256(&self.subjects) {
            Ok(digest) if digest == self.subject_inventory_sha256 => {}
            Ok(_) => violations
                .push("subject_inventory_sha256 does not match the listed subjects".to_string()),
            Err(error) => violations.push(error),
        }
        violations
    }
}
