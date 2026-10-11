//! The lightweight preflight may discharge only the two checks it actually ran.
//! A receipt is bound to the exact current inputs, implementation and toolchain.
use sha2::{Digest, Sha256};
use std::path::Path;
#[path = "../source_identity.rs"]
mod source_identity;
const RECEIPT: &str = "target/ripr/reports/policy-preflight.json";

pub fn enter_workspace_root() -> Result<(), String> {
    let current = std::env::current_dir().map_err(|e| format!("policy workspace root: {e}"))?;
    for root in current.ancestors() {
        let manifest = root.join("Cargo.toml");
        let helper = root.join("tools/repo-policy").is_dir();
        let missing_manifest_boundary = !manifest.exists()
            && root.join("Cargo.lock").exists()
            && root.join("rust-toolchain.toml").exists();
        let workspace = if helper || missing_manifest_boundary {
            true
        } else if manifest.is_file() {
            std::fs::read_to_string(&manifest)
                .map_err(|e| format!("policy workspace root: {}: {e}", manifest.display()))?
                .lines()
                .any(|line| {
                    let header = line
                        .split_once('#')
                        .map_or(line, |(header, _)| header)
                        .trim();
                    header
                        .strip_prefix('[')
                        .and_then(|table| table.strip_suffix(']'))
                        .and_then(|table| table.split('.').next())
                        .is_some_and(|table| {
                            matches!(table.trim(), "workspace" | "\"workspace\"" | "'workspace'")
                        })
                })
        } else {
            false
        };
        if workspace {
            // A damaged nearest workspace must fail its identity check here,
            // rather than falling through to a different enclosing checkout.
            return std::env::set_current_dir(root)
                .map_err(|e| format!("policy workspace root: {}: {e}", root.display()));
        }
    }
    Err("policy workspace root not found from the current directory".into())
}

pub fn verify_executable_identity() -> Result<(), String> {
    let current = source_identity::source_identity(Path::new("."))?;
    if current != env!("REPO_POLICY_SOURCE_ID") {
        return Err("policy executable is stale or belongs to a different source tree; rerun with cargo policy".into());
    }
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = std::process::Command::new(compiler)
        .arg("-vV")
        .output()
        .map_err(|e| format!("policy compiler identity: {e}"))?;
    let compiler = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    if !output.status.success() || compiler.replace('\n', "|") != env!("REPO_POLICY_COMPILER_ID") {
        return Err(
            "policy executable toolchain differs from the current rustc; rerun with cargo policy"
                .into(),
        );
    }
    Ok(())
}

fn input_identity() -> Result<String, String> {
    // Include ignored/untracked policy inputs too: Cargo/git freshness alone
    // cannot notice an ignored workflow created after a successful preflight.
    let mut paths = Vec::new();
    for root in [
        ".github",
        ".agents",
        ".claude",
        "policy",
        "scripts",
        "tools",
        "xtask/src",
        "fixtures/boundary_gap/expected",
        "AGENTS.md",
        "AGENTS.override.md",
        "CLAUDE.md",
        "docs/ARCHITECTURE.md",
    ] {
        let root = Path::new(root);
        if root.exists() {
            paths.extend(crate::collect_files(root)?);
        } else {
            paths.push(root.to_path_buf());
        }
    }
    // The workflow corpus names report/proof paths dynamically. Bind those
    // files even when a fixture points outside the conventional expected tree.
    let corpus_path = Path::new("fixtures/boundary_gap/expected/assistant-loop-health/corpus.json");
    if let Ok(bytes) = std::fs::read(corpus_path)
        && let Ok(corpus) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Some(cases) = corpus["cases"].as_array()
    {
        for case in cases {
            for key in ["expected_report", "expected_markdown"] {
                if let Some(path) = case[key].as_str() {
                    paths.push(path.into());
                }
            }
            if let Some(proofs) = case["proofs"].as_array() {
                for proof in proofs {
                    if let Some(path) = proof.as_str() {
                        paths.push(path.into());
                    }
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    let mut hash = Sha256::new();
    for path in paths {
        let name = crate::normalize_path(&path);
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        // Deletions are inputs too; the policy checks decide whether they are allowed.
        match std::fs::read(&path) {
            Ok(bytes) => {
                hash.update([1]);
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                hash.update([0]);
            }
            Err(e) => return Err(format!("policy input identity: {name}: {e}")),
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn preflight() -> Result<(), String> {
    match std::fs::remove_file(RECEIPT) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("remove stale policy receipt: {e}")),
    }
    verify_executable_identity()?;
    let before = input_identity()?;
    crate::check_workflows_impl()?;
    crate::agent_skills::check()?;
    let after = input_identity()?;
    if before != after {
        return Err("policy inputs changed during preflight".into());
    }
    let receipt = serde_json::json!({"schema":1,"inputs":after,"source":env!("REPO_POLICY_SOURCE_ID"),"compiler":env!("REPO_POLICY_COMPILER_ID"),"checks":["check-workflows","check-agent-skills"]});
    crate::write_report("policy-preflight.json", &receipt.to_string())
}

pub fn verify_preflight(path: &Path) -> Result<(), String> {
    verify_executable_identity()?;
    let receipt = crate::read_json_value(path)?;
    if receipt["schema"] != 1
        || receipt["inputs"] != input_identity()?
        || receipt["source"] != env!("REPO_POLICY_SOURCE_ID")
        || receipt["compiler"] != env!("REPO_POLICY_COMPILER_ID")
        || receipt["checks"] != serde_json::json!(["check-workflows", "check-agent-skills"])
    {
        return Err("policy preflight receipt is stale, malformed or incomplete; rerun cargo policy preflight".into());
    }
    println!(
        "policy preflight: verified check-workflows and check-agent-skills; no duplicate execution"
    );
    Ok(())
}
