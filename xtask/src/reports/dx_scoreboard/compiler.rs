//! Compiler evidence from the Cargo invocation that supplies the measured binary.
//! External binaries and historical receipts never borrow this host's compiler.

#[path = "../../../../crates/ripr/src/build_compiler_record.rs"]
mod record;

use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) struct BuiltBinary {
    pub(super) path: PathBuf,
    pub(super) rustc: Option<String>,
}

/// Cargo emits both fresh artifacts and cached build-script output. Join the
/// exact manifest/target to its package's compiler record, then require the
/// successful build-finished event. Never infer an artifact from target layout.
/// https://doc.rust-lang.org/cargo/reference/external-tools.html
pub(super) fn owned_build(stdout: &str, manifest: &Path) -> Result<BuiltBinary, String> {
    let mut artifacts = Vec::new();
    let mut scripts = Vec::new();
    let mut finished = Vec::new();
    for (index, line) in stdout.lines().enumerate() {
        if index >= 4096 {
            return Err("Cargo build output exceeds the 4096-record evidence limit".to_string());
        }
        if !line.starts_with('{') {
            continue;
        }
        let message: Value = serde_json::from_str(line)
            .map_err(|error| format!("invalid Cargo build evidence: {error}"))?;
        match message["reason"].as_str() {
            Some("compiler-artifact")
                if message["target"]["name"] == "ripr"
                    && message["target"]["kind"] == serde_json::json!(["bin"])
                    && message["profile"]["test"] == false
                    && message["manifest_path"]
                        .as_str()
                        .is_some_and(|path| manifest_matches(Path::new(path), manifest)) =>
            {
                let package = message["package_id"]
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .ok_or("Cargo binary artifact has no package identity")?;
                let executable = message["executable"]
                    .as_str()
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .ok_or("Cargo binary artifact has no absolute executable path")?;
                artifacts.push((package.to_string(), executable));
            }
            Some("build-script-executed") => {
                if let Some(package) = message["package_id"].as_str() {
                    let records: Vec<_> = message["env"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|entry| entry[0] == record::ENV)
                        .collect();
                    let compiler = match records.as_slice() {
                        [entry] if entry.as_array().is_some_and(|pair| pair.len() == 2) => entry[1]
                            .as_str()
                            .filter(|value| record::parse(value).is_some())
                            .map(str::to_string),
                        _ => None,
                    };
                    scripts.push((package.to_string(), compiler));
                }
            }
            Some("build-finished") => finished.push(message["success"].as_bool()),
            _ => (),
        }
    }
    if finished != [Some(true)] || artifacts.len() != 1 {
        return Err(
            "Cargo build must identify one ripr binary artifact and one successful completion"
                .to_string(),
        );
    }
    let (package, path) = artifacts.remove(0);
    let mut matching = scripts.into_iter().filter(|(id, _)| id == &package);
    let rustc = matching.next().and_then(|(_, compiler)| compiler);
    let rustc = if matching.next().is_none() {
        rustc
    } else {
        None
    };
    Ok(BuiltBinary { path, rustc })
}

fn manifest_matches(reported: &Path, expected: &Path) -> bool {
    // The caller canonicalizes its selected manifest. Cargo may spell that
    // same file without Windows' verbatim prefix or through a path alias.
    reported == expected
        || (reported.is_absolute()
            && reported
                .canonicalize()
                .is_ok_and(|resolved| resolved == expected))
}

/// The two comparison paths share this rule; absent identities cannot match.
pub(super) fn comparison_issue(current: Option<&str>, baseline: Option<&str>) -> Option<String> {
    match (
        current.and_then(record::parse),
        baseline.and_then(record::parse),
    ) {
        (Some(now), Some(before)) if now == before => None,
        (Some(_), Some(_)) => Some(format!(
            "build compiler differs (baseline `{}`, current `{}`)",
            description(baseline),
            description(current),
        )),
        _ => Some(
            "build compiler identity is unrecorded or invalid; measured speed is not comparable"
                .to_string(),
        ),
    }
}

pub(super) fn description(identity: Option<&str>) -> String {
    match identity.and_then(record::parse) {
        Some([release, commit, host, llvm]) => {
            format!("rustc {release} ({commit}), {host}, LLVM {llvm}")
        }
        None => "rustc unrecorded".to_string(),
    }
}
