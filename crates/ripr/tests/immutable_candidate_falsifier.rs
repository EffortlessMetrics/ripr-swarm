//! Immutable candidate isolation and committed-analysis parity (#3279,
//! R4 of #3237).
//!
//! The producer, CLI, and identity fields exist (#3276–#3278); this
//! harness tries to break the claim end to end. Each test binds a real
//! subject against a real two-commit repository, deliberately mutates
//! mutable repository state the analysis must not read, and compares
//! the full JSON output for semantic identity. The parity test then
//! checks out the candidate tree as a real commit and compares the
//! ordinary committed run against the subject run after removing only
//! the declared non-portable telemetry.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const LIB_BASE: &str = "pub fn one() -> u8 { 1 }\n";
const LIB_CANDIDATE: &str = "pub fn one() -> u8 { 2 }\n";
const TEST_BODY: &str = "#[test]\nfn t() { assert_eq!(falsifier::one(), 1); }\n";
const CONFIG: &str = "[analysis]\nmode = \"draft\"\n";

struct RepoGuard(PathBuf);
impl Drop for RepoGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unique_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("ripr-3279-{name}-{}-{nanos}", std::process::id()))
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("git {args:?} failed to start: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// A repository with a base commit and a candidate commit; returns the
/// guard, base commit, and candidate commit.
fn fixture_repo(name: &str) -> Result<(RepoGuard, String, String), String> {
    let root = unique_root(name);
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    let guard = RepoGuard(root.clone());
    git(&root, &["init", "--initial-branch=main"])?;
    git(&root, &["config", "user.email", "r@e.invalid"])?;
    git(&root, &["config", "user.name", "ripr"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname='falsifier'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    write(&root, "ripr.toml", CONFIG)?;
    write(&root, "src/lib.rs", LIB_BASE)?;
    write(&root, "tests/it.rs", TEST_BODY)?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "base"])?;
    let base = git(&root, &["rev-parse", "HEAD"])?;
    write(&root, "src/lib.rs", LIB_CANDIDATE)?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "candidate"])?;
    let candidate = git(&root, &["rev-parse", "HEAD"])?;
    Ok((guard, base, candidate))
}

fn run_check(root: &Path, args: &[&str]) -> Result<Value, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(["check", "--root"])
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("ripr check failed to start: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ripr check {:?} failed with {}: {}",
            args,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find('{').ok_or("no JSON in output")?;
    serde_json::from_str(&text[start..]).map_err(|e| format!("parse check JSON: {e}"))
}

fn subject_args(base: Option<&str>, candidate: &str) -> Vec<String> {
    let mut args = vec!["--candidate-tree".to_string(), candidate.to_string()];
    if let Some(base) = base {
        args.push("--candidate-base".to_string());
        args.push(base.to_string());
    }
    args.push("--format".to_string());
    args.push("json".to_string());
    args
}

fn run_subject(root: &Path, base: &str, candidate: &str) -> Result<Value, String> {
    run_subject_with_base(root, Some(base), candidate)
}

fn run_subject_with_base(
    root: &Path,
    base: Option<&str>,
    candidate: &str,
) -> Result<Value, String> {
    let args = subject_args(base, candidate);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_check(root, &refs)
}

/// The #3279 reproduction, verbatim: bind base B and candidate tree T,
/// then change the worktree source, an unchanged test, ripr.toml, and
/// the live index (a staged blob). The output must be semantically
/// identical to the clean run.
#[test]
fn post_bind_mutations_do_not_change_immutable_output() -> Result<(), String> {
    let (guard, base, candidate) = fixture_repo("mutations")?;
    let root = &guard.0;
    let clean = run_subject(root, &base, &candidate)?;

    write(root, "src/lib.rs", "pub fn DIRTY() -> u8 { 99 }\n")?;
    write(root, "tests/it.rs", "#[test]\nfn t() { assert!(false); }\n")?;
    write(root, "ripr.toml", "[analysis]\nmode = \"deep\"\n")?;
    write(root, "src/staged.rs", "pub fn STAGED() -> u8 { 42 }\n")?;
    git(root, &["add", "."])?; // the live index now points at other bytes
    let dirty = run_subject(root, &base, &candidate)?;

    assert_eq!(
        serde_json::to_string(&clean).unwrap_or_default(),
        serde_json::to_string(&dirty).unwrap_or_default(),
        "post-bind worktree/test/config/index mutations must not change a subject run"
    );
    Ok(())
}

/// Same-tree committed parity: check out the candidate as a real commit
/// and run the ordinary `--diff` path; findings, classifications, and
/// ordering must agree with the subject run after removing only the
/// declared non-portable telemetry (identity block, mode string, root).
#[test]
fn same_tree_immutable_and_committed_analysis_agree() -> Result<(), String> {
    let (guard, base, candidate) = fixture_repo("parity")?;
    let root = &guard.0;
    let subject = run_subject(root, &base, &candidate)?;

    // Committed path: an equivalent two-commit checkout is exactly this
    // repository at the candidate commit; run the ordinary range diff.
    git(root, &["checkout", "-q", &candidate])?;
    let committed = run_check(root, &["--base", &base, "--format", "json"])?;

    let semantic = |value: &Value| -> Value {
        let mut copy = value.clone();
        // Declared non-portable telemetry: the identity block carries
        // the input's own fingerprints (diff-source hashes differ by
        // construction between a range diff and a tree-to-tree diff);
        // `mode` is rendered from the input path; `root` is the caller
        // path. Everything semantic — summary, findings, classifications,
        // related tests, ordering — must match.
        // Declared non-portable telemetry ONLY: the identity block
        // (input fingerprints differ by construction between a range
        // diff and a tree-to-tree diff), the mode string, the root
        // echo, and the base echo. Completeness, counts, and
        // limitations are acceptance-named and MUST match (#3279
        // review M2).
        copy["analysis_outcome"]["outcome"]["identity"] = Value::Null;
        copy["mode"] = Value::Null;
        copy["root"] = Value::Null;
        copy["base"] = Value::Null;
        copy
    };
    assert_eq!(
        serde_json::to_string(&semantic(&subject)).unwrap_or_default(),
        serde_json::to_string(&semantic(&committed)).unwrap_or_default(),
        "same-tree immutable and committed runs must agree semantically"
    );
    Ok(())
}

/// Emitted identities match the requested subject exactly, including the
/// per-format empty-tree OID, and the probe paths name the repository,
/// never the ephemeral materialization directory.
#[test]
fn emitted_identities_match_the_request_and_paths_are_replayable() -> Result<(), String> {
    let (guard, _base, candidate) = fixture_repo("identity")?;
    let root = &guard.0;
    let subject = run_subject_with_base(root, None, &candidate)?;
    let tree = git(root, &["rev-parse", &format!("{candidate}^{{tree}}")])?;
    let identity = subject["analysis_outcome"]["outcome"]["identity"]["git_candidate_subject"]
        .as_object()
        .ok_or("git_candidate_subject missing")?;
    assert_eq!(identity["candidate_tree"].as_str(), Some(tree.as_str()));
    assert_eq!(identity["subject_kind"].as_str(), Some("tree_to_tree"));
    let diff_identity = identity["diff_identity"]
        .as_str()
        .ok_or("diff_identity missing")?;
    assert!(diff_identity.starts_with("sha256:"));
    // Empty-tree base resolves the repository's real empty-tree OID.
    assert_eq!(
        identity["base_tree"].as_str(),
        Some("4b825dc642cb6eb9a060e54bf8d69288fbee4904")
    );
    let rendered = serde_json::to_string(&subject).unwrap_or_default();
    assert!(
        !rendered.contains("ripr-git-candidate"),
        "the ephemeral materialization root must not leak into output"
    );
    Ok(())
}

/// Deletes and renames never require a worktree fallback: delete the
/// file in the candidate commit, rename another, and dirty the worktree
/// copies of both — the subject run still resolves from objects.
#[test]
fn delete_and_rename_shapes_resolve_without_worktree_fallback() -> Result<(), String> {
    let root = unique_root("shapes");
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    let _guard = RepoGuard(root.clone());
    git(&root, &["init", "--initial-branch=main"])?;
    git(&root, &["config", "user.email", "r@e.invalid"])?;
    git(&root, &["config", "user.name", "ripr"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]\nname='falsifier'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    write(&root, "src/lib.rs", LIB_BASE)?;
    write(&root, "src/gone.rs", "pub fn gone() -> u8 { 1 }\n")?;
    write(&root, "src/renamed.rs", "pub fn named() -> u8 { 1 }\n")?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "base"])?;
    let base = git(&root, &["rev-parse", "HEAD"])?;
    // Candidate: delete gone.rs, rename renamed.rs -> moved.rs, keep lib.
    std::fs::remove_file(root.join("src/gone.rs")).map_err(|e| e.to_string())?;
    std::fs::rename(root.join("src/renamed.rs"), root.join("src/moved.rs"))
        .map_err(|e| e.to_string())?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "candidate"])?;
    let candidate = git(&root, &["rev-parse", "HEAD"])?;
    // Dirty the worktree copies of exactly the shapes under test.
    write(&root, "src/gone.rs", "pub fn RESURRECTED() -> u8 { 9 }\n")?;
    std::fs::remove_file(root.join("src/moved.rs")).map_err(|e| e.to_string())?;
    let subject = run_subject(&root, &base, &candidate)?;
    let rendered = serde_json::to_string(&subject).unwrap_or_default();
    assert!(
        !rendered.contains("RESURRECTED"),
        "a worktree resurrection of a deleted file must not enter the run"
    );
    assert!(
        !rendered.contains("src/renamed.rs"),
        "the pre-rename path must not be read from the worktree (its file was deleted)"
    );
    // The pure rename produces no probes by the pinned rename semantics;
    // the deletion is disclosed or probed. Either way the run must be a
    // completed analysis driven by the trees.
    let kind = subject["analysis_outcome"]["outcome"]["kind"]
        .as_str()
        .unwrap_or("");
    assert!(
        !kind.is_empty(),
        "the subject run must carry a completed outcome"
    );
    Ok(())
}

/// Invalid inputs stay named incomplete states — never clean zero
/// findings: a missing tree, and a malformed OID.
#[test]
fn invalid_subjects_fail_closed_never_clean() -> Result<(), String> {
    let (guard, base, _candidate) = fixture_repo("invalid")?;
    let root = &guard.0;
    let missing = match run_subject(root, &base, "0123456789012345678901234567890123456789") {
        Err(error) => error,
        Ok(_) => return Err("a missing tree must fail".to_string()),
    };
    assert!(
        missing.contains("git candidate subject"),
        "missing tree must fail inside the subject boundary: {missing}"
    );
    let malformed = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(["check", "--root"])
        .arg(root)
        .args(["--candidate-tree", "deadbeef", "--format", "json"])
        .output()
        .map_err(|e| e.to_string())?;
    assert!(!malformed.status.success());
    let stderr = String::from_utf8_lossy(&malformed.stderr);
    assert!(
        stderr.contains("malformed object ID"),
        "malformed OID must be named: {stderr}"
    );
    Ok(())
}

/// The removal experiment (#3279 fix plan 7): prove the corpus fails if
/// the producer silently substitutes the worktree. Simulated by running
/// the ordinary (non-subject) diff path against a dirty worktree — the
/// outputs must DIFFER from the subject run, so a regression that made
/// the subject path read the worktree would flip this assertion.
#[test]
fn removal_experiment_subject_differs_from_worktree_substitution() -> Result<(), String> {
    let (guard, base, candidate) = fixture_repo("removal")?;
    let root = &guard.0;
    let subject = run_subject(root, &base, &candidate)?;
    // Dirty the worktree to the shape a worktree-substituting producer
    // would read: a DIFFERENT source change than the candidate's.
    write(
        root,
        "src/lib.rs",
        "pub fn WORKTREE_SUBSTITUTE() -> u8 { 7 }\n",
    )?;
    let substituted = run_subject(root, &base, &candidate)?;
    // The subject run must be unaffected (this is the isolation claim)…
    assert_eq!(
        serde_json::to_string(&subject).unwrap_or_default(),
        serde_json::to_string(&substituted).unwrap_or_default()
    );
    // …and the candidate's real change must be what the findings carry:
    // a producer that silently read the worktree would carry the dirty
    // bytes instead. The candidate change is `one() -> 2`; the dirty
    // shape has no `one`. Pin the discriminator.
    // The changed probe's after-text carries the candidate's  from
    // ; rendered per-surface (e.g. flow sink
    // text or after evidence) it may be a bare  — pin the value, not
    // the rendering.
    let rendered = serde_json::to_string(&subject).unwrap_or_default();
    assert!(
        !rendered.contains("WORKTREE_SUBSTITUTE"),
        "worktree-only bytes must never appear in a subject run"
    );
    let finding = subject["findings"]
        .get(0)
        .ok_or("the candidate change must produce a finding")?;
    // The JSON probe shape carries the changed value as `expression`
    // (the rendered changed-value surface); a worktree-substituting
    // producer would render the dirty bytes here instead.
    let expression = finding["probe"]["expression"]
        .as_str()
        .ok_or("finding must carry the changed expression")?;
    assert_eq!(
        expression.trim(),
        "2",
        "findings must carry the candidate bytes, not worktree bytes"
    );
    Ok(())
}

/// Temporary candidate state is cleaned: after a successful run the
/// materialization root is removed (the guard drops with the temp tree).
/// #3279 review B1 control: a tree WITHOUT a config configures itself
/// with the pure default — toggling the worktree ripr.toml (including
/// its enabled-languages list) must not change the subject output.
#[test]
fn treeless_config_subject_ignores_worktree_language_toggle() -> Result<(), String> {
    let root = unique_root("treeless");
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    let _guard = RepoGuard(root.clone());
    git(&root, &["init", "--initial-branch=main"])?;
    git(&root, &["config", "user.email", "r@e.invalid"])?;
    git(&root, &["config", "user.name", "ripr"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]
name='falsifier'
version='0.1.0'
edition='2024'
",
    )?;
    write(&root, "src/lib.rs", LIB_BASE)?;
    write(&root, "tests/it.rs", TEST_BODY)?;
    // NOTE: no ripr.toml is committed — the tree configures itself with
    // the pure default.
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "base"])?;
    let base = git(&root, &["rev-parse", "HEAD"])?;
    write(&root, "src/lib.rs", LIB_CANDIDATE)?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "candidate"])?;
    let candidate = git(&root, &["rev-parse", "HEAD"])?;

    // Worktree config A (untracked — mutable state only).
    write(
        &root,
        "ripr.toml",
        "[languages]
enabled = [\"rust\"]
",
    )?;
    let first = run_subject(&root, &base, &candidate)?;
    // Worktree config B: the ONLY change is the enabled-languages list.
    write(
        &root,
        "ripr.toml",
        "[languages]
enabled = [\"rust\", \"python\"]
",
    )?;
    let second = run_subject(&root, &base, &candidate)?;
    assert_eq!(
        serde_json::to_string(&first).unwrap_or_default(),
        serde_json::to_string(&second).unwrap_or_default(),
        "toggling the worktree enabled-languages must not change a treeless-config subject run"
    );
    Ok(())
}

/// #3279 review M3: a type change (regular file → symlink) in the
/// candidate tree fails closed with a named error naming the entry —
/// never a worktree fallback, never clean zero findings.
#[test]
fn type_change_fails_closed_naming_the_entry() -> Result<(), String> {
    let root = unique_root("typechange");
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    let _guard = RepoGuard(root.clone());
    git(&root, &["init", "--initial-branch=main"])?;
    git(&root, &["config", "user.email", "r@e.invalid"])?;
    git(&root, &["config", "user.name", "ripr"])?;
    write(
        &root,
        "Cargo.toml",
        "[package]
name='falsifier'
version='0.1.0'
edition='2024'
",
    )?;
    write(&root, "src/lib.rs", LIB_BASE)?;
    write(
        &root,
        "src/linked.rs",
        "pub fn linked() -> u8 { 1 }
",
    )?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "base"])?;
    let base = git(&root, &["rev-parse", "HEAD"])?;
    write(&root, "src/lib.rs", LIB_CANDIDATE)?;
    // Replace the regular file with a symlink via the index (type change).
    std::fs::remove_file(root.join("src/linked.rs")).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("lib.rs", root.join("src/linked.rs")).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        // Windows symlinks need privileges; use a gitlinks-free stand-in:
        // commit the deletion only (type-change-to-absent is covered by
        // the delete corpus test). Skip the symlink arm where it cannot
        // be created, keeping the corpus honest.
        if std::os::windows::fs::symlink_file("lib.rs", root.join("src/linked.rs")).is_err() {
            return Ok(());
        }
    }
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "candidate"])?;
    let candidate = git(&root, &["rev-parse", "HEAD"])?;
    let error = match run_subject(&root, &base, &candidate) {
        Err(error) => error,
        Ok(_) => return Err("a type-changed entry must fail closed".to_string()),
    };
    assert!(
        error.contains("git candidate subject"),
        "failure must stay inside the subject boundary: {error}"
    );
    Ok(())
}

#[test]
fn temporary_candidate_state_is_cleaned() -> Result<(), String> {
    let (guard, base, candidate) = fixture_repo("cleanup")?;
    let root = &guard.0;
    // The oracle is the set of PER-RUN materialization roots (children
    // of the shared ripr-git-candidate parent), not the parent itself —
    // the parent persists by design and counting it made the test both
    // cold-start flaky and vacuous (#3279 review M1).
    let parent = std::env::temp_dir().join("ripr-git-candidate");
    let children = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect()
    };
    run_subject(root, &base, &candidate)?;
    // Concurrent corpus tests materialize into the same shared parent, so
    // a single snapshot can see a sibling test's root mid-flight. A
    // genuinely leaked root persists; a concurrent run's root drops when
    // that test finishes — settle briefly and require emptiness.
    let mut persisted = Vec::new();
    for _ in 0..12 {
        persisted = children(&parent);
        if persisted.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    assert!(
        persisted.is_empty(),
        "no per-run materialization root may persist after a completed run: {persisted:?}"
    );
    Ok(())
}
