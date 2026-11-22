//! Request, registry classification and the fail-closed next step (ripr#1649).
//!
//! crates.io publication is not atomic across packages and a published
//! version can never be replaced. `decide` therefore proposes at most one
//! upload, only for a package the registry does not have, only after every
//! package before it in dependency order is public with the exact bytes, and
//! never twice under one authorization.

use crate::blind_journey::sha256_hex;
use crate::release_github_transaction::model::{is_hex, is_stable_version};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) const REQUEST_SCHEMA: &str = "ripr.release.crates_io_transaction.request.v1";
pub(crate) const PUBLISH_OPERATION: &str = "crates_io_publish";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageNode {
    pub(crate) name: String,
    pub(crate) version: String,
    /// SHA-256 of the exact `.crate` archive to upload.
    pub(crate) crate_sha256: String,
    pub(crate) size: u64,
    /// First-party packages in this set that this one depends on.
    pub(crate) requires: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CratesIoTransactionRequest {
    pub(crate) schema: String,
    pub(crate) registry: String,
    pub(crate) candidate_sha: String,
    pub(crate) candidate_tree: String,
    pub(crate) ship_packet_sha256: String,
    pub(crate) stable_decision_sha256: String,
    pub(crate) package_set_sha256: String,
    /// Dependency order: every package follows the packages it requires.
    pub(crate) packages: Vec<PackageNode>,
}

pub(crate) fn package_set_sha256(packages: &[PackageNode]) -> Result<String, String> {
    serde_json::to_string(packages)
        .map(|json| sha256_hex(json.as_bytes()))
        .map_err(|error| format!("package set serialization failed: {error}"))
}

fn valid_crate_name(name: &str) -> bool {
    name.len() <= 64
        && name.starts_with(|first: char| first.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// crates.io treats `-` and `_` and letter case as the same name.
fn registry_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

impl CratesIoTransactionRequest {
    /// Identity of this exact request, as bare lowercase hex. The
    /// authorization packet names it; the request does not name the packet.
    pub(crate) fn request_id(&self) -> Result<String, String> {
        serde_json::to_string(self)
            .map(|json| sha256_hex(json.as_bytes()))
            .map_err(|error| format!("request serialization failed: {error}"))
    }

    pub(crate) fn violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        if self.schema != REQUEST_SCHEMA {
            violations.push("unsupported request schema".to_string());
        }
        if self.registry != "crates.io" {
            violations.push("registry must be crates.io".to_string());
        }
        if !is_hex(&self.candidate_sha, 40) || !is_hex(&self.candidate_tree, 40) {
            violations
                .push("candidate_sha and candidate_tree must be 40 lowercase hex".to_string());
        }
        for (name, value) in [
            ("ship_packet_sha256", &self.ship_packet_sha256),
            ("stable_decision_sha256", &self.stable_decision_sha256),
            ("package_set_sha256", &self.package_set_sha256),
        ] {
            if !is_hex(value, 64) {
                violations.push(format!("{name} must be 64 lowercase hex"));
            }
        }
        if self.packages.is_empty() {
            violations.push("packages must not be empty".to_string());
        }
        let mut earlier = BTreeSet::new();
        for package in &self.packages {
            let name = &package.name;
            if !valid_crate_name(name) {
                violations.push(format!("`{name}` is not a crate name"));
            }
            if !is_stable_version(&package.version) {
                violations.push(format!("`{name}` version must be MAJOR.MINOR.PATCH"));
            }
            if package.size == 0 || !is_hex(&package.crate_sha256, 64) {
                violations.push(format!(
                    "`{name}` needs a nonzero size and 64-hex crate_sha256"
                ));
            }
            let mut required = BTreeSet::new();
            for dependency in &package.requires {
                if !required.insert(registry_name(dependency)) {
                    violations.push(format!("`{name}` requires `{dependency}` twice"));
                }
                if !earlier.contains(&registry_name(dependency)) {
                    violations.push(format!(
                        "`{name}` requires `{dependency}`, which is not an earlier package in the set"
                    ));
                }
            }
            if !earlier.insert(registry_name(name)) {
                violations.push(format!("`{name}` is listed twice"));
            }
        }
        match package_set_sha256(&self.packages) {
            Ok(digest) if digest == self.package_set_sha256 => {}
            Ok(_) => {
                violations.push("package_set_sha256 does not match the listed packages".to_string())
            }
            Err(error) => violations.push(error),
        }
        violations
    }
}

/// What the release authorization verifier (ripr#1651) established.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifiedAuthorization {
    pub(crate) authorization_id: String,
    pub(crate) transaction_request_id: String,
    pub(crate) allowed_operations: BTreeSet<String>,
    /// `(crate file name, sha256)` of every subject the packet names.
    pub(crate) subjects: Vec<(String, String)>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub(crate) enum RegistryObservation {
    /// The registry answered that this exact name and version does not exist.
    Absent,
    /// The version exists. `downloaded_sha256` is the hash of the bytes
    /// actually downloaded, or `None` when they could not be read.
    Present {
        yanked: bool,
        downloaded_sha256: Option<String>,
    },
    /// Timeout, rate limit, propagation delay or any other non-answer.
    Unavailable { reason: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageObservation {
    pub(crate) name: String,
    pub(crate) registry: RegistryObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegistryInventory {
    pub(crate) packages: Vec<PackageObservation>,
    /// Packages for which an upload was already attempted under this
    /// authorization, whatever the reported outcome. Matched by registry
    /// name (`-`/`_` and letter case ignored); a name outside the request's
    /// set rejects the inventory.
    pub(crate) attempted: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PackageState {
    AbsentAdmitted,
    ExactExisting,
    ImmutableConflict,
    HoldAmbiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AggregateState {
    NothingPublished,
    PartialPublicationHold,
    PackageSetPublishedExact,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "step")]
pub(crate) enum Step {
    Publish {
        name: String,
    },
    /// Every package is public with the exact bytes. Nothing is uploaded.
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StopKind {
    RequestRejected,
    AuthorizationMismatch,
    NotAuthorized,
    /// The inventory names an attempt for a package outside the request.
    InventoryRejected,
    RegistryUnavailable,
    ImmutableConflict,
    /// An upload was already attempted under this authorization and the
    /// registry still does not show the package. A retry needs a successor
    /// authorization; it is never inferred from failure.
    AttemptConsumed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Stop {
    pub(crate) kind: StopKind,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub(crate) enum Decision {
    Proceed { step: Step },
    Stop { stop: Stop },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Planned {
    pub(crate) request_id: Option<String>,
    pub(crate) authorization_id: String,
    /// One entry per package, in request order.
    pub(crate) packages: Vec<(String, PackageState)>,
    pub(crate) aggregate: AggregateState,
    pub(crate) decision: Decision,
}

fn stop(kind: StopKind, detail: impl Into<String>) -> Decision {
    Decision::Stop {
        stop: Stop {
            kind,
            detail: detail.into(),
        },
    }
}

fn classify(package: &PackageNode, inventory: &RegistryInventory) -> PackageState {
    let mut observed = inventory
        .packages
        .iter()
        .filter(|observation| observation.name == package.name);
    let (Some(observation), None) = (observed.next(), observed.next()) else {
        // No observation, or two that could disagree.
        return PackageState::HoldAmbiguous;
    };
    match &observation.registry {
        RegistryObservation::Absent => PackageState::AbsentAdmitted,
        RegistryObservation::Unavailable { .. } => PackageState::HoldAmbiguous,
        RegistryObservation::Present {
            downloaded_sha256: None,
            ..
        } => PackageState::HoldAmbiguous,
        RegistryObservation::Present {
            yanked,
            downloaded_sha256: Some(sha256),
        } => {
            // A yanked version is public and immutable but does not resolve
            // for a dependent, so it is not the exact published state.
            if !*yanked && *sha256 == package.crate_sha256 {
                PackageState::ExactExisting
            } else {
                PackageState::ImmutableConflict
            }
        }
    }
}

/// Decide the next step from one fresh registry inventory.
pub(crate) fn decide(
    request: &CratesIoTransactionRequest,
    authorization: &VerifiedAuthorization,
    inventory: &RegistryInventory,
) -> Planned {
    let planned = |request_id, packages: Vec<(String, PackageState)>, decision: Decision| {
        let exact = packages
            .iter()
            .filter(|(_, state)| *state == PackageState::ExactExisting)
            .count();
        let aggregate = if !packages.is_empty() && exact == packages.len() {
            AggregateState::PackageSetPublishedExact
        } else if exact > 0 {
            // Some packages are public and others are not, whether or not
            // the next upload is admitted.
            AggregateState::PartialPublicationHold
        } else {
            AggregateState::NothingPublished
        };
        Planned {
            request_id,
            authorization_id: authorization.authorization_id.clone(),
            packages,
            aggregate,
            decision,
        }
    };
    let violations = request.violations();
    if !violations.is_empty() {
        return planned(
            None,
            Vec::new(),
            stop(StopKind::RequestRejected, violations.join("; ")),
        );
    }
    let request_id = match request.request_id() {
        Ok(id) => id,
        Err(error) => return planned(None, Vec::new(), stop(StopKind::RequestRejected, error)),
    };
    if authorization.transaction_request_id != request_id {
        return planned(
            Some(request_id),
            Vec::new(),
            stop(
                StopKind::AuthorizationMismatch,
                "the authorization names another transaction request",
            ),
        );
    }
    let mut expected: Vec<(String, String)> = request
        .packages
        .iter()
        .map(|package| {
            (
                format!("{}-{}.crate", package.name, package.version),
                package.crate_sha256.clone(),
            )
        })
        .collect();
    expected.sort();
    let mut authorized_subjects = authorization.subjects.clone();
    authorized_subjects.sort();
    if authorized_subjects != expected {
        return planned(
            Some(request_id),
            Vec::new(),
            stop(
                StopKind::AuthorizationMismatch,
                "the authorization names a different package set",
            ),
        );
    }
    let in_set: BTreeSet<String> = request
        .packages
        .iter()
        .map(|package| registry_name(&package.name))
        .collect();
    let attempted: BTreeSet<String> = inventory
        .attempted
        .iter()
        .map(|name| registry_name(name))
        .collect();
    if let Some(foreign) = attempted.difference(&in_set).next() {
        return planned(
            Some(request_id),
            Vec::new(),
            stop(
                StopKind::InventoryRejected,
                format!("the attempt record names `{foreign}`, which is not in the package set"),
            ),
        );
    }
    let states: Vec<(String, PackageState)> = request
        .packages
        .iter()
        .map(|package| (package.name.clone(), classify(package, inventory)))
        .collect();
    // Any conflict anywhere in the set stops before the first upload: a set
    // that cannot complete must not start.
    if let Some((name, _)) = states
        .iter()
        .find(|(_, state)| *state == PackageState::ImmutableConflict)
    {
        let detail =
            format!("`{name}` is public with other bytes or is yanked; it is never replaced");
        return planned(
            Some(request_id),
            states,
            stop(StopKind::ImmutableConflict, detail),
        );
    }
    // An unknown state anywhere can hide a conflict, so it also stops before
    // any upload: the conflict rule is only as strong as the observation.
    if let Some((name, _)) = states
        .iter()
        .find(|(_, state)| *state == PackageState::HoldAmbiguous)
    {
        let detail = format!("the registry state of `{name}` could not be established");
        return planned(
            Some(request_id),
            states,
            stop(StopKind::RegistryUnavailable, detail),
        );
    }
    let decision = match states
        .iter()
        .find(|(_, state)| *state != PackageState::ExactExisting)
    {
        None => Decision::Proceed {
            step: Step::Complete,
        },
        Some((name, _)) if attempted.contains(&registry_name(name)) => stop(
            StopKind::AttemptConsumed,
            format!("an upload of `{name}` was already attempted under this authorization"),
        ),
        Some((_, _)) if !authorization.allowed_operations.contains(PUBLISH_OPERATION) => stop(
            StopKind::NotAuthorized,
            format!("the authorization does not allow {PUBLISH_OPERATION}"),
        ),
        Some((name, _)) => Decision::Proceed {
            step: Step::Publish { name: name.clone() },
        },
    };
    planned(Some(request_id), states, decision)
}
