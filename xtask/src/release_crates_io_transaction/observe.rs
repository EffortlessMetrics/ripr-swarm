//! Read-only observation of one package set on the registry (ripr#1649).
//!
//! Two independent sources must agree before a version is called absent or
//! present: the registry API and the sparse index a dependent resolves
//! through. Disagreement, any other status, or no answer is "unavailable".
//! A present version is reported by the SHA-256 of its downloaded archive.

use super::plan::{
    CratesIoTransactionRequest, PackageNode, PackageObservation, RegistryInventory,
    RegistryObservation,
};
use crate::blind_journey::sha256_hex;
use serde_json::Value;
use std::collections::BTreeSet;
use std::process::Command;

const API: &str = "https://crates.io/api/v1/crates";
const INDEX: &str = "https://index.crates.io";
const ARCHIVES: &str = "https://static.crates.io/crates";
const USER_AGENT: &str = "ripr-xtask-release-crates-io-transaction-observe";

pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
}

pub(crate) trait RegistryRead {
    /// GET one https URL. `Err` means no HTTP answer was obtained.
    fn get(&mut self, url: &str) -> Result<Response, String>;
}

/// Split transfer output that ends in the three-digit HTTP status.
pub(crate) fn split_status(stdout: &[u8]) -> Option<Response> {
    let (body, status) = stdout.split_last_chunk::<3>()?;
    let status = std::str::from_utf8(status).ok()?.parse().ok()?;
    (100..=599).contains(&status).then(|| Response {
        status,
        body: body.to_vec(),
    })
}

/// The process transport: https only, GET only, no redirects, no shell.
pub(crate) struct HttpsGet;

impl RegistryRead for HttpsGet {
    fn get(&mut self, url: &str) -> Result<Response, String> {
        let output = Command::new("curl")
            .args(["--silent", "--show-error", "--proto", "=https"])
            .args(["--max-time", "120", "--user-agent", USER_AGENT])
            .args(["--write-out", "%{http_code}", url])
            .output()
            .map_err(|error| format!("failed to start the transfer: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let reason = stderr.lines().next().unwrap_or("no output");
            return Err(format!("GET {url} gave no HTTP answer: {reason}"));
        }
        split_status(&output.stdout).ok_or_else(|| format!("GET {url} gave no HTTP status"))
    }
}

/// Sparse-index path of a crate name, which the index stores lowercase.
pub(crate) fn index_path(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}

/// What one source says about one exact version.
enum Listed {
    Absent,
    Present { yanked: bool, checksum: String },
}

fn api_says(api: &mut dyn RegistryRead, package: &PackageNode) -> Result<Listed, String> {
    let url = format!("{API}/{}/{}", package.name, package.version);
    let response = api.get(&url)?;
    let body: Value = serde_json::from_slice(&response.body)
        .map_err(|error| format!("GET {url} answered unparseable JSON: {error}"))?;
    match response.status {
        // Only the registry's own not-found answer counts, not any 404.
        404 if body["errors"].as_array().is_some_and(|e| !e.is_empty()) => Ok(Listed::Absent),
        200 => {
            let version = &body["version"];
            if version["crate"].as_str() != Some(&package.name)
                || version["num"].as_str() != Some(&package.version)
            {
                // The registry folds `-`/`_` and case; another spelling of
                // this name is not the requested package.
                return Err(format!("GET {url} answered another crate or version"));
            }
            match (version["yanked"].as_bool(), version["checksum"].as_str()) {
                (Some(yanked), Some(checksum)) => Ok(Listed::Present {
                    yanked,
                    checksum: checksum.to_string(),
                }),
                _ => Err(format!("GET {url} answered no yanked flag or checksum")),
            }
        }
        status => Err(format!("GET {url} answered HTTP {status}")),
    }
}

fn index_says(api: &mut dyn RegistryRead, package: &PackageNode) -> Result<Listed, String> {
    let url = format!("{INDEX}/{}", index_path(&package.name));
    let response = api.get(&url)?;
    match response.status {
        // The index has no file for a crate that was never published.
        404 => return Ok(Listed::Absent),
        200 => {}
        status => return Err(format!("GET {url} answered HTTP {status}")),
    }
    let text = std::str::from_utf8(&response.body)
        .map_err(|error| format!("GET {url} answered non-UTF-8: {error}"))?;
    let mut found = None;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let entry: Value = serde_json::from_str(line)
            .map_err(|error| format!("GET {url} answered an unparseable line: {error}"))?;
        let (Some(name), Some(version)) = (entry["name"].as_str(), entry["vers"].as_str()) else {
            return Err(format!("GET {url} answered a line with no name or version"));
        };
        if name != package.name {
            return Err(format!("GET {url} answered another crate, `{name}`"));
        }
        if version != package.version {
            continue;
        }
        let (Some(yanked), Some(checksum)) = (entry["yanked"].as_bool(), entry["cksum"].as_str())
        else {
            return Err(format!("GET {url} answered no yanked flag or checksum"));
        };
        if found.replace((yanked, checksum.to_string())).is_some() {
            return Err(format!("GET {url} lists {version} twice"));
        }
    }
    Ok(match found {
        None => Listed::Absent,
        Some((yanked, checksum)) => Listed::Present { yanked, checksum },
    })
}

fn observe_package(api: &mut dyn RegistryRead, package: &PackageNode) -> RegistryObservation {
    let unavailable = |reason: String| RegistryObservation::Unavailable { reason };
    let listed = match (api_says(api, package), index_says(api, package)) {
        (Err(reason), _) | (_, Err(reason)) => return unavailable(reason),
        (Ok(by_api), Ok(by_index)) => (by_api, by_index),
    };
    let (yanked, checksum) = match listed {
        (Listed::Absent, Listed::Absent) => return RegistryObservation::Absent,
        // Propagation in either direction: neither absent nor resolvable.
        (Listed::Absent, Listed::Present { .. }) | (Listed::Present { .. }, Listed::Absent) => {
            return unavailable("the registry API and the index disagree on presence".to_string());
        }
        (
            Listed::Present { yanked, checksum },
            Listed::Present {
                yanked: index_yanked,
                checksum: index_checksum,
            },
        ) => {
            if checksum != index_checksum {
                return unavailable("the registry API and the index disagree on bytes".to_string());
            }
            // Yanked in either source means a dependent may not resolve it.
            (yanked || index_yanked, checksum)
        }
    };
    let name = &package.name;
    let url = format!("{ARCHIVES}/{name}/{name}-{}.crate", package.version);
    let downloaded_sha256 = match api.get(&url) {
        Ok(response) if response.status == 200 => {
            let sha256 = sha256_hex(&response.body);
            // Bytes that do not match what the registry itself lists were
            // not read correctly; they prove nothing about the package.
            (sha256 == checksum).then_some(sha256)
        }
        _ => None,
    };
    RegistryObservation::Present {
        yanked,
        downloaded_sha256,
    }
}

/// One fresh read of every package in the request. The attempt record is not
/// a registry fact and is always empty here; the upload side owns it.
pub(crate) fn observe(
    api: &mut dyn RegistryRead,
    request: &CratesIoTransactionRequest,
) -> RegistryInventory {
    RegistryInventory {
        packages: request
            .packages
            .iter()
            .map(|package| PackageObservation {
                name: package.name.clone(),
                registry: observe_package(api, package),
            })
            .collect(),
        attempted: BTreeSet::new(),
    }
}
