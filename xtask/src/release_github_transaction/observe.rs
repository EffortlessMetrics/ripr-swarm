//! Read-only observation of one request's tag and release state (ripr#1644).
//!
//! Every read is a GET. Only an HTTP 404 from a repository this token can
//! write to is "absent"; any other answer, or no answer, is "unavailable".
//! An asset is reported by the SHA-256 of the bytes actually downloaded, and
//! its provenance is verified over those same bytes.

use super::inventory::{
    AttestationObservation, AttestationState, BytesObservation, ReleasesObservation, RemoteAsset,
    RemoteInventory, RemoteRelease, TagObservation,
};
use super::model::{ExpectedSubject, GitHubReleaseTransactionRequest, normalized_name};
use crate::blind_journey::sha256_hex;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

const JSON: &str = "application/vnd.github+json";
const BYTES: &str = "application/octet-stream";
const PAGE: usize = 100;
/// A listing longer than this is reported unavailable rather than truncated.
const MAX_PAGES: usize = 50;

pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
}

pub(crate) trait GitHubRead {
    /// GET one REST path. `Err` means no HTTP answer was obtained.
    fn get(&mut self, path: &str, accept: &str) -> Result<Response, String>;
}

/// Split `gh api --include` output into status and body. `None` when the
/// output does not start with an HTTP status line and a complete header block.
pub(crate) fn parse_included(stdout: &[u8]) -> Option<Response> {
    let line_end = stdout.iter().position(|byte| *byte == b'\n')?;
    let status_line = std::str::from_utf8(&stdout[..line_end]).ok()?;
    let mut words = status_line.split_ascii_whitespace();
    if !words.next()?.starts_with("HTTP/") {
        return None;
    }
    let status = words.next()?.parse().ok()?;
    let headers_end = stdout.windows(4).position(|window| window == b"\r\n\r\n")?;
    Some(Response {
        status,
        body: stdout[headers_end + 4..].to_vec(),
    })
}

/// The `gh` transport: fixed host, GET only, no shell.
pub(crate) struct GhCli;

impl GitHubRead for GhCli {
    fn get(&mut self, path: &str, accept: &str) -> Result<Response, String> {
        let accept = format!("Accept: {accept}");
        let output = Command::new("gh")
            .args(["api", "--hostname", "github.com", "--method", "GET"])
            .args(["--include", "-H", &accept, path])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?;
        // gh exits nonzero for an HTTP error but still prints the answer, so
        // the status line decides, not the exit code.
        parse_included(&output.stdout).ok_or_else(|| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let reason = stderr.lines().next().unwrap_or("no output");
            format!("GET {path} gave no HTTP answer: {reason}")
        })
    }
}

/// `Ok(None)` is an HTTP 404. Any other non-200 answer is an error.
fn json(api: &mut dyn GitHubRead, path: &str) -> Result<Option<Value>, String> {
    let response = api.get(path, JSON)?;
    match response.status {
        200 => serde_json::from_slice(&response.body)
            .map(Some)
            .map_err(|error| format!("GET {path} answered unparseable JSON: {error}")),
        404 => Ok(None),
        status => Err(format!("GET {path} answered HTTP {status}")),
    }
}

/// Every element of a paginated array endpoint, or an error if any page is
/// missing, is not an array, or the listing does not end within the bound.
fn list(api: &mut dyn GitHubRead, path: &str) -> Result<Vec<Value>, String> {
    let mut items = Vec::new();
    for page in 1..=MAX_PAGES {
        let paged = format!("{path}?per_page={PAGE}&page={page}");
        let Some(Value::Array(listed)) = json(api, &paged)? else {
            return Err(format!("GET {paged} did not answer a list"));
        };
        let count = listed.len();
        items.extend(listed);
        if count < PAGE {
            return Ok(items);
        }
    }
    Err(format!("GET {path} did not end within {MAX_PAGES} pages"))
}

/// A 404 means "absent" only when this token could see the object if it
/// existed. GitHub answers 404 for a repository the token cannot read, and
/// hides draft releases from a token that cannot write.
fn require_write_visibility(api: &mut dyn GitHubRead, repository: &str) -> Result<(), String> {
    let path = format!("repos/{repository}");
    let Some(repo) = json(api, &path)? else {
        return Err(format!("GET {path} answered HTTP 404"));
    };
    if repo["full_name"].as_str() != Some(repository) {
        return Err(format!("GET {path} answered another repository"));
    }
    if repo["permissions"]["push"].as_bool() != Some(true) {
        return Err("this token cannot list draft releases (no push permission)".to_string());
    }
    Ok(())
}

fn observe_tag(
    api: &mut dyn GitHubRead,
    request: &GitHubReleaseTransactionRequest,
) -> TagObservation {
    let unavailable = |reason: String| TagObservation::Unavailable { reason };
    let repository = &request.repository;
    let path = format!("repos/{repository}/git/ref/tags/{}", request.tag);
    let reference = match json(api, &path) {
        Err(reason) => return unavailable(reason),
        Ok(None) => return TagObservation::Absent,
        Ok(Some(reference)) => reference,
    };
    let object = &reference["object"];
    let (Some(sha), Some(kind)) = (object["sha"].as_str(), object["type"].as_str()) else {
        return unavailable(format!("GET {path} answered no object"));
    };
    if reference["ref"].as_str() != Some(&format!("refs/tags/{}", request.tag)) {
        return unavailable(format!("GET {path} answered another ref"));
    }
    if kind != "tag" {
        // A ref straight at a commit (or anything else) is not annotated.
        // A lightweight tag's object is the commit itself.
        return TagObservation::Present {
            object_sha: sha.to_string(),
            peeled_sha: sha.to_string(),
            annotated: false,
        };
    }
    let tag_path = format!("repos/{repository}/git/tags/{sha}");
    let target = match json(api, &tag_path) {
        Err(reason) => return unavailable(reason),
        Ok(None) => return unavailable(format!("GET {tag_path} answered HTTP 404")),
        Ok(Some(tag_object)) => tag_object["object"].clone(),
    };
    match (target["sha"].as_str(), target["type"].as_str()) {
        // A tag of a tag is reported at its inner object and not as an
        // annotated tag of a commit, so it can never classify as exact.
        (Some(target_sha), Some(target_kind)) => TagObservation::Present {
            object_sha: sha.to_string(),
            peeled_sha: target_sha.to_string(),
            annotated: target_kind == "commit",
        },
        _ => unavailable(format!("GET {tag_path} answered no target")),
    }
}

fn observe_asset(
    api: &mut dyn GitHubRead,
    repository: &str,
    asset: &Value,
    expected: &BTreeSet<String>,
) -> Result<(RemoteAsset, Option<Vec<u8>>), String> {
    let (Some(id), Some(name), Some(size)) = (
        asset["id"].as_u64(),
        asset["name"].as_str(),
        asset["size"].as_u64(),
    ) else {
        return Err("a release asset has no id, name or size".to_string());
    };
    let unavailable = |reason: String| BytesObservation::Unavailable { reason };
    let mut downloaded = None;
    let bytes = if !expected.contains(&normalized_name(name)) {
        // Unexpected assets are a conflict by name alone.
        unavailable("not an expected subject; not downloaded".to_string())
    } else if asset["state"].as_str() != Some("uploaded") {
        unavailable("the asset upload is not complete".to_string())
    } else {
        let path = format!("repos/{repository}/releases/assets/{id}");
        match api.get(&path, BYTES) {
            Err(reason) => unavailable(reason),
            Ok(response) if response.status != 200 => {
                unavailable(format!("GET {path} answered HTTP {}", response.status))
            }
            Ok(response) if response.body.len() as u64 != size => unavailable(format!(
                "GET {path} answered {} bytes, listed size {size}",
                response.body.len()
            )),
            Ok(response) => {
                let sha256 = sha256_hex(&response.body);
                downloaded = Some(response.body);
                BytesObservation::Downloaded { sha256 }
            }
        }
    };
    let asset = RemoteAsset {
        id,
        name: name.to_string(),
        size,
        bytes,
    };
    Ok((asset, downloaded))
}

struct ObservedReleases {
    releases: Vec<RemoteRelease>,
    /// `(asset name, bytes)` of every expected asset that downloaded.
    downloads: Vec<(String, Vec<u8>)>,
}

fn observe_releases(
    api: &mut dyn GitHubRead,
    request: &GitHubReleaseTransactionRequest,
) -> Result<ObservedReleases, String> {
    let repository = &request.repository;
    let expected: BTreeSet<String> = request
        .subjects
        .iter()
        .map(|subject| normalized_name(&subject.name))
        .collect();
    let mut bound = Vec::new();
    let mut downloads = Vec::new();
    for release in list(api, &format!("repos/{repository}/releases"))? {
        let (Some(id), Some(tag), Some(draft), Some(prerelease)) = (
            release["id"].as_u64(),
            release["tag_name"].as_str(),
            release["draft"].as_bool(),
            release["prerelease"].as_bool(),
        ) else {
            return Err("a listed release has no id, tag, draft or prerelease".to_string());
        };
        // A case-variant tag name is kept so it classifies as a conflict.
        if !tag.eq_ignore_ascii_case(&request.tag) {
            continue;
        }
        // The embedded asset list is not relied on; the asset endpoint is paged.
        let mut assets = Vec::new();
        for asset in list(api, &format!("repos/{repository}/releases/{id}/assets"))? {
            let (asset, downloaded) = observe_asset(api, repository, &asset, &expected)?;
            if let Some(bytes) = downloaded {
                downloads.push((asset.name.clone(), bytes));
            }
            assets.push(asset);
        }
        bound.push(RemoteRelease {
            id,
            tag: tag.to_string(),
            draft,
            prerelease,
            title: release["name"].as_str().unwrap_or_default().to_string(),
            body_sha256: sha256_hex(release["body"].as_str().unwrap_or_default().as_bytes()),
            assets,
        });
    }
    Ok(ObservedReleases {
        releases: bound,
        downloads,
    })
}

pub(crate) trait ProvenanceCheck {
    /// Whether `bytes` carry a source provenance attestation for this
    /// subject under the request's repository, workflow, ref and candidate.
    fn verify(
        &mut self,
        request: &GitHubReleaseTransactionRequest,
        subject: &ExpectedSubject,
        bytes: &[u8],
    ) -> AttestationState;
}

/// Whether `gh attestation verify --format json` output names this subject
/// with this digest in a verified statement. The command's own flags bind
/// repository, workflow, ref and candidate; this binds the subject's name.
pub(crate) fn names_subject(stdout: &[u8], subject: &ExpectedSubject) -> bool {
    let Ok(Value::Array(verified)) = serde_json::from_slice::<Value>(stdout) else {
        return false;
    };
    verified.iter().any(|attestation| {
        attestation["verificationResult"]["statement"]["subject"]
            .as_array()
            .is_some_and(|subjects| {
                subjects.iter().any(|named| {
                    named["name"].as_str() == Some(&subject.name)
                        && named["digest"]["sha256"].as_str() == Some(&subject.sha256)
                })
            })
    })
}

/// `gh attestation verify` over the downloaded bytes, written to a private
/// temporary directory under the subject's own name.
pub(crate) struct GhAttestation;

impl ProvenanceCheck for GhAttestation {
    fn verify(
        &mut self,
        request: &GitHubReleaseTransactionRequest,
        subject: &ExpectedSubject,
        bytes: &[u8],
    ) -> AttestationState {
        let directory = std::env::temp_dir().join(format!(
            "ripr-release-provenance-{}-{}",
            std::process::id(),
            subject.sha256
        ));
        let file = directory.join(&subject.name);
        let written = fs::create_dir(&directory).and_then(|()| fs::write(&file, bytes));
        let output = written.and_then(|()| {
            Command::new("gh")
                .args(["attestation", "verify"])
                .arg(&file)
                .args(["--hostname", "github.com", "--format", "json"])
                .args(["--repo", &request.repository])
                .args(["--signer-workflow", &request.provenance.signer_workflow])
                .args(["--source-ref", &request.provenance.source_ref])
                .args(["--source-digest", &request.candidate_sha])
                .args(["--deny-self-hosted-runners"])
                .output()
        });
        // Best effort: a leftover file holds only bytes that are already public.
        let _removed = fs::remove_dir_all(&directory);
        match output {
            Ok(output) if output.status.success() && names_subject(&output.stdout, subject) => {
                AttestationState::Verified
            }
            // gh does not separate "no attestation" from "could not check".
            _ => AttestationState::Unavailable,
        }
    }
}

/// One fresh read of everything `decide` needs. Provenance is checked only
/// for a subject whose exact bytes were downloaded from exactly one asset of
/// that exact name; every other subject is `unavailable`.
pub(crate) fn observe(
    api: &mut dyn GitHubRead,
    provenance: &mut dyn ProvenanceCheck,
    request: &GitHubReleaseTransactionRequest,
) -> RemoteInventory {
    let unchecked = || {
        request
            .subjects
            .iter()
            .map(|subject| AttestationObservation {
                subject: subject.name.clone(),
                state: AttestationState::Unavailable,
            })
            .collect::<Vec<_>>()
    };
    if let Err(reason) = require_write_visibility(api, &request.repository) {
        return RemoteInventory {
            tag: TagObservation::Unavailable {
                reason: reason.clone(),
            },
            releases: ReleasesObservation::Unavailable { reason },
            attestations: unchecked(),
        };
    }
    let tag = observe_tag(api, request);
    let ObservedReleases {
        releases,
        downloads,
    } = match observe_releases(api, request) {
        Ok(observed) => observed,
        Err(reason) => {
            return RemoteInventory {
                tag,
                releases: ReleasesObservation::Unavailable { reason },
                attestations: unchecked(),
            };
        }
    };
    let attestations = request
        .subjects
        .iter()
        .map(|subject| {
            let mut exact = downloads.iter().filter(|(name, bytes)| {
                *name == subject.name && sha256_hex(bytes) == subject.sha256
            });
            let state = match (exact.next(), exact.next()) {
                (Some((_, bytes)), None) => provenance.verify(request, subject, bytes),
                _ => AttestationState::Unavailable,
            };
            AttestationObservation {
                subject: subject.name.clone(),
                state,
            }
        })
        .collect();
    RemoteInventory {
        tag,
        releases: ReleasesObservation::Listed { releases },
        attestations,
    }
}
