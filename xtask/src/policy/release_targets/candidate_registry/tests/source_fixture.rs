//! Actual separate source/controller fixture for the #4510 admission seam.
//! This characterizes registry machinery; it does not qualify an installation.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::{
    CandidateOperation, PINNED_JSON, RegistryDocument, controllers,
    evaluate_candidate_registry, pinned_artifact, read_artifact_tree,
    render_projection, repository_registry, resolve_candidate_authority,
    retire_template, sha256_hex, tree,
};

struct OwnedFixture {
    root: PathBuf,
    finished: bool,
}

impl OwnedFixture {
    fn finish(mut self) -> Result<(), String> {
        fs::remove_dir_all(&self.root)
            .map_err(|error| format!("owned fixture cleanup failed: {error}"))?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for OwnedFixture {
    fn drop(&mut self) {
        if !self.finished {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn git(root: &Path, values: &[&str]) -> Result<String, String> {
    let args = values.iter().map(|value| (*value).to_string()).collect::<Vec<_>>();
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        Path::new("git"), &args, root,
        &[], &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
        Duration::from_secs(30), "4510 separate-source fixture Git",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!("fixture Git {values:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|error| format!("fixture Git output is not UTF-8: {error}"))
}

fn initialize(root: &Path) -> Result<(), String> {
    git(root, &["init", "--initial-branch=main"])?;
    for (key, value) in [
        ("user.name", "RIPR fixture"), ("user.email", "fixture@example.invalid"),
        ("core.autocrlf", "false"), ("core.fsmonitor", "false"),
        ("commit.gpgsign", "false"), ("tag.gpgsign", "false"),
        ("core.hooksPath", ".fixture-empty-hooks"),
    ] {
        git(root, &["config", "--local", key, value])?;
    }
    Ok(())
}

fn replace_row(registry: &mut Value, artifact: &[u8], sha: &str, tree_id: &str, reference: &str)
    -> Result<(), String>
{
    let row = registry.get_mut("artifacts").and_then(Value::as_array_mut)
        .and_then(|rows| rows.iter_mut().find(|row| row.get("path") == Some(&json!(PINNED_JSON))))
        .ok_or_else(|| "legal fixture has no candidate row".to_string())?;
    let object = row.as_object_mut()
        .ok_or_else(|| "legal candidate row is not an object".to_string())?;
    object.insert("sha256".to_string(), json!(sha256_hex(artifact)));
    object.insert("candidate".to_string(), json!({"sha": sha, "tree": tree_id, "ref": reference}));
    Ok(())
}

#[test]
fn registered_source_fixture_precedes_separate_controller_commit() -> Result<(), String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|error| format!("fixture clock: {error}"))?.as_nanos();
    let owned = std::env::temp_dir().join(format!("ripr-4510-source-{}-{stamp}", std::process::id()));
    fs::create_dir(&owned).map_err(|error| format!("exclusive fixture root: {error}"))?;
    let guard = OwnedFixture { root: owned, finished: false };
    let source = guard.root.join("source é");
    let controller = guard.root.join("controller with spaces");
    fs::create_dir(&source).map_err(|error| error.to_string())?;
    fs::create_dir(&controller).map_err(|error| error.to_string())?;
    initialize(&source)?;
    fs::create_dir(source.join("src")).map_err(|error| error.to_string())?;
    fs::write(source.join("Cargo.toml"),
        "[package]\nname='ripr'\nversion='0.11.0'\nedition='2024'\n")
        .map_err(|error| error.to_string())?;
    fs::write(source.join("src/main.rs"), "fn main() { println!(\"fixture source A\"); }\n")
        .map_err(|error| error.to_string())?;
    git(&source, &["add", "--", "Cargo.toml", "src/main.rs"])?;
    git(&source, &["commit", "-m", "fixture candidate before controller"])?;
    let sha = git(&source, &["rev-parse", "HEAD"])?;
    let tree_id = git(&source, &["rev-parse", "HEAD^{tree}"])?;
    let reference = format!("refs/tags/ripr-release-0.11.0-{sha}");
    git(&source, &["update-ref", &reference, &sha])?;
    if git(&source, &["rev-parse", &reference])? != sha {
        return Err("fixture ref does not name the actual source commit".to_string());
    }

    let mut artifact: Value = serde_json::from_slice(&pinned_artifact())
        .map_err(|error| error.to_string())?;
    artifact.as_object_mut()
        .ok_or_else(|| "legal candidate artifact is not an object".to_string())?
        .insert("selected_swarm_parent".to_string(), json!(sha));
    let artifact = serde_json::to_vec_pretty(&artifact).map_err(|error| error.to_string())?;
    let mut registry = retire_template(repository_registry());
    replace_row(&mut registry, &artifact, &sha, &tree_id, &reference)?;
    let fixture = tree(&registry, &[(PINNED_JSON, artifact.clone())]);
    // Write the actual controller snapshot, including its derived projection.
    let document: RegistryDocument = serde_json::from_value(registry)
        .map_err(|error| error.to_string())?;
    if render_projection(&document.artifacts).is_empty() {
        return Err("fixture projection unexpectedly empty".to_string());
    }
    initialize(&controller)?;
    for (path, bytes) in &fixture.files {
        let destination = controller.join(path);
        let parent = destination.parent().ok_or_else(|| "fixture path has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::write(destination, bytes).map_err(|error| error.to_string())?;
    }
    git(&controller, &["add", "--", "docs"])?;
    git(&controller, &["commit", "-m", "control packet after source selection"])?;
    if git(&controller, &["rev-parse", "HEAD"])? == sha {
        return Err("controller must be distinct from immutable source commit".to_string());
    }
    let readback = read_artifact_tree(&controller);
    let outcome = evaluate_candidate_registry(&readback, &controllers());
    let validated = outcome.validated().map_err(|error|
        format!("legal actual-source fixture invalid: {error}; {:?}", outcome.violations))?;
    let granted = resolve_candidate_authority(
        &validated, "0.11.0", &artifact, CandidateOperation::ExactCandidate,
    )?;
    if granted.candidate_sha.as_deref() != Some(sha.as_str()) {
        return Err("registry did not retain actual candidate SHA".to_string());
    }
    if !git(&source, &["status", "--porcelain=v1", "--untracked-files=all"] )?.is_empty() {
        return Err("candidate source unexpectedly dirty".to_string());
    }
    let mut changed = artifact;
    changed.push(b' ');
    if resolve_candidate_authority(&validated, "0.11.0", &changed, CandidateOperation::ExactCandidate).is_ok() {
        return Err("changed actual controller artifact bytes admitted".to_string());
    }
    guard.finish()
}
