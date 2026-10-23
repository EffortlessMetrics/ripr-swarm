//! Proposal serialization is deliberately separate from accepted admission.
use super::{input, retain_manifest, safe_artifact_path};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const USAGE: &str = "release-manifest-proposal --controller-root <path> --version <version> --input <relative-json> --out <new-relative-directory>";

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut flags = BTreeMap::new();
    let mut pairs = args.chunks_exact(2);
    for pair in &mut pairs {
        let flag = pair[0].as_str();
        if !["--controller-root", "--version", "--input", "--out"].contains(&flag)
            || flags.insert(flag, pair[1].as_str()).is_some()
        {
            return Err(format!("unknown or repeated {flag}; {USAGE}"));
        }
    }
    if !pairs.remainder().is_empty() || flags.len() != 4 {
        return Err(USAGE.to_string());
    }
    let get = |flag| flags.get(flag).copied().ok_or_else(|| USAGE.to_string());
    let root = Path::new(get("--controller-root")?)
        .canonicalize()
        .map_err(|error| format!("resolve proposal controller root: {error}"))?;
    let relative = get("--input")?;
    let bytes = input::read_owned(&root, relative, input::MAX_RETAINED_BYTES)?;
    // This returns validated input custody, never an admitted snapshot. No
    // producer-computed digest is used as an independent acceptance decision.
    let manifest = retain_manifest(&root, get("--version")?, relative, bytes)?.manifest;
    let json = format!(
        "{}\n",
        serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("serialize manifest proposal: {error}"))?
    );
    if json.len() as u64 > input::MAX_INPUT_BYTES {
        return Err("serialized manifest proposal exceeds the 16 MiB input budget".to_string());
    }
    // The entire typed serialization is the handoff. Choose a fence that no
    // input string can close, including a non-claim containing Markdown.
    let longest = json.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.saturating_add(1).max(3));
    let markdown = format!(
        "# Swarm candidate manifest proposal\n\nIndependent #1609 acceptance is not established.\n\nProducer: xtask {} `release-manifest-proposal`; manifest schema {}.\nProposed JSON SHA256: `{}`.\n\nThis is a schema-1.1 serialization and retained-input check. It does not verify\nthe final main cut, native owner decisions, live ref protection, or qualification.\nEvidence paths remain relative to the original controller root.\nThe complete typed manifest follows; post-freeze executions remain required_not_run.\n\n{fence}json\n{json}{fence}\n",
        env!("CARGO_PKG_VERSION"),
        manifest.schema_version,
        super::digest(json.as_bytes())
    );
    let output = Path::new(get("--out")?);
    if !safe_artifact_path(output) {
        return Err(
            "proposal output must be an ordinary controller-relative directory".to_string(),
        );
    }
    let output = root.join(output);
    let parent = output
        .parent()
        .ok_or_else(|| "proposal output has no parent".to_string())?
        .canonicalize()
        .map_err(|error| format!("resolve proposal output parent: {error}"))?;
    if !parent.starts_with(&root) {
        return Err("proposal output parent escapes the controller root".to_string());
    }
    // Exclusivity preserves all earlier proposals and inputs. A partial write
    // is an error with its directory retained, not a successful transaction.
    fs::create_dir(&output).map_err(|error| format!("create new proposal directory: {error}"))?;
    for (name, bytes) in [
        ("manifest.json", json.as_bytes()),
        ("manifest.md", markdown.as_bytes()),
    ] {
        let result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(name))
            .and_then(|mut file| file.write_all(bytes));
        result.map_err(|error| {
            format!(
                "proposal write failed; retain incomplete {}: {error}",
                output.display()
            )
        })?;
    }
    println!("Manifest proposal written; independent #1609 acceptance is not established.");
    Ok(())
}

#[cfg(test)]
mod tests;
