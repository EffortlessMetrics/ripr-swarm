//! Actual public Python journey consumer. Installed custody is the only
//! production entry; workspace replay is explicitly a test preparation scope.
use super::super::CommandResult;
use super::{CandidateExecution, archive::InstalledCandidate};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

mod history;
mod receipts;
mod runtime;
#[cfg(test)]
mod tests;
mod workflow;

#[path = "../../../../../crates/ripr/tests/common/printed_words.rs"]
mod printed_words;

pub(crate) const TARGET: &str = "tests/test_pricing.py";
const FILES: [&str; 4] = [
    "pyproject.toml",
    "pricing/__init__.py",
    "pricing/core.py",
    TARGET,
];
// The live card selects StrengthenExistingTest and names the last test in
// the retained file. Append its missing equality assertion inside that test.
const EDIT: &str = "    assert bulk_discount_rate(20) == 0.15\n";

pub(crate) fn observe(candidate: &InstalledCandidate, parent: &Path) -> Result<Value, String> {
    candidate.revalidate()?;
    let execution = CandidateExecution::Qualified(candidate);
    let result = journey(
        parent,
        &candidate.python_journey_manifest()?,
        |snapshot, path| candidate.python_journey_file(snapshot, path),
        |root, args, date| match date {
            Some(date) => {
                execution.fixture_git_at(root, args, date, "Python pinned fixture history")
            }
            None => execution.fixture_git(root, args, "Python fixture Git"),
        },
        |args, cwd, context| execution.run(args, cwd, context),
    );
    candidate.revalidate()?;
    result.map(|mut result| {
        result["candidate_custody"] = candidate.custody_json();
        result
    })
}

fn journey(
    parent: &Path,
    manifest: &[u8],
    mut read: impl FnMut(&str, &str) -> Result<Vec<u8>, String>,
    mut git: impl FnMut(&Path, &[&str], Option<&str>) -> Result<CommandResult, String>,
    mut run: impl FnMut(&[String], &Path, &str) -> Result<CommandResult, String>,
) -> Result<Value, String> {
    let result = (|| {
        fs::create_dir(parent).map_err(|e| format!("exclusive Python evidence root: {e}"))?;
        let parent = parent.canonicalize().map_err(|e| e.to_string())?;
        let root = parent.join("selected ' é");
        let history = history::prepare(&root, manifest, &mut read, &mut git)?;
        let foreign = parent.join("foreign decoy");
        fs::create_dir(&foreign).map_err(|e| e.to_string())?;
        git_ok(
            &mut git,
            &foreign,
            &["init", "--quiet", "--template=", "--initial-branch=main"],
            None,
        )?;
        fs::write(
            foreign.join("pyproject.toml"),
            "[project]\nname='decoy'\nversion='0.0.0'\n",
        )
        .map_err(|e| e.to_string())?;
        let decoy = inventory(&foreign)?;
        let result = workflow::follow(&root, &foreign, &parent, &history, &mut git, &mut run)?;
        if inventory(&foreign)? != decoy {
            return Err("Python journey changed the foreign decoy".into());
        }
        Ok(
            json!({"schema_version":"installed_python_journey_preparation.v1",
            "scope":"preparation_only", "history":history,"journey":result,
            "decoy_unchanged":true,"installed_acceptance":"not_established",
            "blind_acceptance":"not_established",
            "non_claims":["No central immutable candidate or final language matrix exists.",
                "Workspace real-runtime replay and synthetic package custody tests are separate proof planes.",
                "Caller-reported project status is not authenticated execution authority.",
                "Native Windows and interrupted-write qualification remain unestablished."]}),
        )
    })();
    result.map_err(|error: String| {
        format!("{error}; Python evidence retained at {}", parent.display())
    })
}

fn git_ok(
    git: &mut impl FnMut(&Path, &[&str], Option<&str>) -> Result<CommandResult, String>,
    root: &Path,
    args: &[&str],
    date: Option<&str>,
) -> Result<CommandResult, String> {
    let result = git(root, args, date)?;
    if !result.success {
        return Err(format!("Python fixture Git {args:?}: {}", result.stderr));
    }
    Ok(result)
}

fn invoke(
    run: &mut impl FnMut(&[String], &Path, &str) -> Result<CommandResult, String>,
    root: &Path,
    evidence: &Path,
    commands: &mut Vec<Value>,
    args: Vec<String>,
    context: &str,
) -> Result<CommandResult, String> {
    let result = run(&args, root, context)?;
    let row = json!({"argv":args,"cwd":root,"context":context,"exit":result.status,
        "stdout":result.stdout,"stderr":result.stderr});
    write_json(
        &evidence.join(format!("public-command-{}.json", commands.len())),
        &row,
    )?;
    commands.push(row);
    Ok(result)
}

fn require_success(result: &CommandResult, context: &str) -> Result<(), String> {
    if result.success && result.status == Some(0) {
        return Ok(());
    }
    Err(format!(
        "{context}: exit {:?}\n{}\n{}",
        result.status, result.stdout, result.stderr
    ))
}

fn plain_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() || meta.len() > 16 * 1024 * 1024 {
        return Err(format!(
            "not an ordinary bounded Python artifact: {}",
            path.display()
        ));
    }
    fs::read(path).map_err(|e| e.to_string())
}
fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&plain_bytes(path)?).map_err(|e| e.to_string())
}
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn artifact(path: &Path) -> Result<Value, String> {
    let bytes = plain_bytes(path)?;
    Ok(json!({"path":path,"bytes":bytes.len(),"sha256":digest(&bytes)}))
}
fn relative_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.contains('\\')
        || relative.contains(':')
        || !Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        || relative
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(format!("unsafe Python artifact path: {relative}"));
    }
    Ok(root.join(relative))
}
fn inventory(root: &Path) -> Result<Value, String> {
    fn walk(
        root: &Path,
        path: &Path,
        rows: &mut std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.is_dir() {
                walk(root, &path, rows)?;
            } else if meta.is_file() {
                rows.insert(
                    path.strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .into_owned(),
                    digest(&plain_bytes(&path)?),
                );
            } else {
                return Err("unexpected link in Python fixture inventory".into());
            }
        }
        Ok(())
    }
    let mut rows = std::collections::BTreeMap::new();
    walk(root, root, &mut rows)?;
    serde_json::to_value(rows).map_err(|e| e.to_string())
}
