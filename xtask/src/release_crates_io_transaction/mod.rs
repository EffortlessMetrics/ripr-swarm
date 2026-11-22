//! Exact, retry-safe crates.io publication of one package set (ripr#1649).
//!
//! A typed request naming the ordered package set, classification of
//! observed registry state per package, and the single next step.
//! `release-crates-io-transaction-plan` evaluates supplied files.
//! `release-crates-io-transaction-observe` reads the registry with GET
//! requests only and prints the inventory the plan consumes. Nothing here
//! uploads, and nothing here holds a credential.

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

/// `release-crates-io-transaction-plan`: print the decision for supplied
/// files. Exit is nonzero for a stop.
pub(crate) fn plan_command(args: &[String]) -> Result<(), String> {
    let paths = paths(args)?;
    let path = |name: &str| paths.get(name).map(String::as_str).unwrap_or_default();
    let request: plan::CratesIoTransactionRequest = read_json(path("--request"), "request")?;
    let authorization: plan::VerifiedAuthorization =
        read_json(path("--authorization"), "authorization")?;
    let inventory: plan::RegistryInventory = read_json(path("--inventory"), "inventory")?;
    let planned = plan::decide(&request, &authorization, &inventory);
    let rendered = serde_json::to_string_pretty(&planned)
        .map_err(|error| format!("render decision: {error}"))?;
    println!("{rendered}");
    match planned.decision {
        plan::Decision::Proceed { .. } => Ok(()),
        plan::Decision::Stop { stop } => Err(format!(
            "release-crates-io-transaction-plan: stop ({:?}): {}",
            stop.kind, stop.detail
        )),
    }
}

/// `release-crates-io-transaction-observe`: read every package of the request
/// from the registry and print the inventory. GET requests only.
pub(crate) fn observe_command(args: &[String]) -> Result<(), String> {
    let [flag, path] = args else {
        return Err("usage: release-crates-io-transaction-observe --request <request.json>".into());
    };
    if flag != "--request" {
        return Err(format!("unexpected argument `{flag}`"));
    }
    let request: plan::CratesIoTransactionRequest = read_json(path, "request")?;
    let violations = request.violations();
    if !violations.is_empty() {
        // A rejected request is never turned into registry reads.
        return Err(format!("request rejected: {}", violations.join("; ")));
    }
    let inventory = observe::observe(&mut observe::HttpsGet, &request);
    let rendered = serde_json::to_string_pretty(&inventory)
        .map_err(|error| format!("render inventory: {error}"))?;
    println!("{rendered}");
    Ok(())
}
