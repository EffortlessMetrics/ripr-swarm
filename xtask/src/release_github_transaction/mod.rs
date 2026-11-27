//! Exact, no-clobber, resumable GitHub tag/release publication (ripr#1644).
//!
//! A typed request, classification of observed remote state, and the single
//! next step. `release-github-transaction-plan` evaluates supplied files.
//! `release-github-transaction-observe` only reads GitHub (GET requests and
//! `gh attestation verify`) and prints the inventory the plan consumes.
//! Nothing here mutates remote state; the write side is a later slice.

mod inventory;
pub(crate) mod model;
mod observe;
#[cfg(test)]
mod observe_tests;
mod plan;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn read_json<T: serde::de::DeserializeOwned>(path: &str, what: &str) -> Result<T, String> {
    let bytes =
        fs::read(Path::new(path)).map_err(|error| format!("read {what} {path}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {what} {path}: {error}"))
}

/// Exactly the three `--flag <path>` pairs, each once, in any order.
fn paths(args: &[String]) -> Result<BTreeMap<&'static str, String>, String> {
    const FLAGS: [&str; 3] = ["--request", "--authorization", "--inventory"];
    let mut found = BTreeMap::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let Some(name) = FLAGS.iter().find(|flag| *flag == arg) else {
            return Err(format!("unexpected argument `{arg}`"));
        };
        let value = rest.next().ok_or_else(|| format!("{name} needs a path"))?;
        if found.insert(*name, value.clone()).is_some() {
            return Err(format!("{name} given more than once"));
        }
    }
    match FLAGS.iter().find(|flag| !found.contains_key(*flag)) {
        Some(missing) => Err(format!("missing {missing} <path>")),
        None => Ok(found),
    }
}

/// `release-github-transaction-plan`: print the decision for supplied files.
/// Exit is nonzero for a stop, so a caller cannot read a refusal as a plan.
pub(crate) fn plan_command(args: &[String]) -> Result<(), String> {
    let paths = paths(args)?;
    let path = |name: &str| paths.get(name).map(String::as_str).unwrap_or_default();
    let request: model::GitHubReleaseTransactionRequest = read_json(path("--request"), "request")?;
    let authorization: plan::VerifiedAuthorization =
        read_json(path("--authorization"), "authorization")?;
    let inventory: inventory::RemoteInventory = read_json(path("--inventory"), "inventory")?;
    let planned = plan::decide(&request, &authorization, &inventory);
    let rendered = serde_json::to_string_pretty(&planned)
        .map_err(|error| format!("render decision: {error}"))?;
    println!("{rendered}");
    match planned.decision {
        plan::Decision::Proceed { .. } => Ok(()),
        plan::Decision::Stop { stop } => Err(format!(
            "release-github-transaction-plan: stop ({:?}): {}",
            stop.kind, stop.detail
        )),
    }
}

/// `release-github-transaction-observe`: read the request's tag and release
/// state from GitHub and print the inventory. It only reads.
pub(crate) fn observe_command(args: &[String]) -> Result<(), String> {
    let [flag, path] = args else {
        return Err("usage: release-github-transaction-observe --request <request.json>".into());
    };
    if flag != "--request" {
        return Err(format!("unexpected argument `{flag}`"));
    }
    let request: model::GitHubReleaseTransactionRequest = read_json(path, "request")?;
    let violations = request.violations();
    if !violations.is_empty() {
        // A rejected request is never turned into remote reads.
        return Err(format!("request rejected: {}", violations.join("; ")));
    }
    let inventory = observe::observe(&mut observe::GhCli, &mut observe::GhAttestation, &request);
    let rendered = serde_json::to_string_pretty(&inventory)
        .map_err(|error| format!("render inventory: {error}"))?;
    println!("{rendered}");
    Ok(())
}
