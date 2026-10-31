//! Live producer replay for the exact-cut schema sweep (#3920).
//!
//! `schema-producer-sweep` validates committed producer bytes on its own, but
//! five published-schema rows have no committed bytes: `doctor`, `pr-evidence`,
//! `review-comments`, `repair-attempt` and `execution_result` need a real
//! producer run. The required gate feeds three of them from its own run; this
//! module runs all five against one scratch repository so a qualification run
//! can replay the whole packet with one command instead of reconstructing the
//! recipe from prose.
//!
//! Identity is bound twice. The scratch repository is built from files read
//! with `git show REV:path`, never from the working tree, and the `ripr`
//! binary must report the same full commit as `REV`, so bytes from a binary
//! built at a different commit cannot be validated as the cut's output.
//!
//! The scratch repository and every output live under
//! `target/ripr/cut-replay/`. Outputs stay outside the scratch checkout on
//! purpose: a repair attempt treats an untracked file inside the checkout as
//! an edit and fails terminally.

use std::fs;
use std::path::Path;
use std::process::Command;

const WORK_DIR: &str = "target/ripr/cut-replay";
const FIXTURE: &str = "fixtures/boundary_gap";
const FIXTURE_FILES: &[&str] = &["Cargo.toml", "src/lib.rs", "tests/pricing.rs"];
/// The line the fixture's `diff.patch` changes (`>` becomes `>=`).
const FIXTURE_SEAM: &str = "src/lib.rs:2";
/// A boundary test that discriminates the fixture's changed predicate.
const BOUNDARY_TEST: &str = "\n#[test]\nfn discounted_total_boundary_discriminator() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n";

/// Runs every live producer on `rev` and returns the `SCHEMA=FILE[#POINTER]`
/// values for `--artifact`, one per runtime-only or authority row.
pub(crate) fn produce(root: &Path, rev: &str, ripr: &Path) -> Result<Vec<String>, String> {
    let commit = git_text(
        root,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )?;
    let ripr = &if ripr.is_absolute() {
        ripr.to_path_buf()
    } else {
        root.join(ripr)
    };
    if !ripr.is_file() {
        return Err(format!(
            "--ripr {} is not a file; build it first",
            ripr.display()
        ));
    }
    let version = capture(ripr, root, &["--version"])?;
    check_identity(&version, &commit, ripr, rev)?;

    let work = root.join(WORK_DIR);
    if work.exists() {
        fs::remove_dir_all(&work).map_err(|e| format!("clear {}: {e}", work.display()))?;
    }
    let out = work.join("artifacts");
    fs::create_dir_all(&out).map_err(|e| format!("create {}: {e}", out.display()))?;

    let patch = git_blob(root, &commit, &format!("{FIXTURE}/diff.patch"))?;
    let patch_path = work.join("diff.patch");
    fs::write(&patch_path, patch).map_err(|e| format!("write diff.patch: {e}"))?;

    // pr-evidence, review-comments, repair-attempt: one repository whose base
    // commit has the pre-change predicate and whose head commit is the fixture.
    let repo = work.join("repo");
    seed_repo(root, &commit, &repo)?;
    git(&repo, &["apply", "-R", &patch_path.to_string_lossy()])?;
    git(&repo, &["add", "-A"])?;
    git_commit(&repo, "base")?;
    let base = git_text(&repo, &["rev-parse", "HEAD"])?;
    git(&repo, &["apply", &patch_path.to_string_lossy()])?;
    git(&repo, &["add", "-A"])?;
    git_commit(&repo, "head")?;
    let head = git_text(&repo, &["rev-parse", "HEAD"])?;

    let doctor = out.join("doctor.json");
    write(&doctor, &capture(ripr, root, &["doctor", "--json"])?)?;

    run(
        ripr,
        &repo,
        &["pr-evidence", "--base", &base, "--head", &head],
    )?;
    let review = out.join("review-comments.json");
    run(
        ripr,
        &repo,
        &[
            "review-comments",
            "--base",
            &base,
            "--head",
            &head,
            "--out",
            &review.to_string_lossy(),
        ],
    )?;
    // Copy out of the checkout before the repair attempt; the attempt's
    // before-phase snapshot would otherwise count them as edits.
    let evidence = out.join("pr-evidence.json");
    fs::copy(repo.join("target/ripr/pr/repo-exposure.json"), &evidence)
        .map_err(|e| format!("pr-evidence wrote no repo-exposure.json: {e}"))?;

    run(
        ripr,
        &repo,
        &[
            "agent",
            "repair",
            "--root",
            ".",
            "--phase",
            "before",
            "--seam-id",
            FIXTURE_SEAM,
        ],
    )?;
    let attempt_id = sole_attempt(&repo)?;
    let test_path = repo.join("tests/pricing.rs");
    let mut test = fs::read_to_string(&test_path).map_err(|e| format!("read pricing.rs: {e}"))?;
    test.push_str(BOUNDARY_TEST);
    write(&test_path, &test)?;
    run(
        ripr,
        &repo,
        &[
            "agent",
            "repair",
            "--root",
            ".",
            "--attempt",
            &attempt_id,
            "--phase",
            "after",
        ],
    )?;
    let attempt = out.join("repair-attempt.json");
    fs::copy(
        repo.join("target/ripr/repair-attempts")
            .join(&attempt_id)
            .join("attempt.json"),
        &attempt,
    )
    .map_err(|e| format!("repair wrote no attempt.json: {e}"))?;

    // execution_result: a producer-generated packet executed through the CLI.
    let verify = work.join("verify");
    seed_repo(root, &commit, &verify)?;
    git(&verify, &["add", "-A"])?;
    git_commit(&verify, "fixture")?;
    let before = capture(
        ripr,
        &verify,
        &["check", "--root", ".", "--format", "repo-exposure-json"],
    )?;
    fs::create_dir_all(verify.join("target/ripr/pilot")).map_err(|e| format!("pilot dir: {e}"))?;
    write(
        &verify.join("target/ripr/pilot/repo-exposure.json"),
        &before,
    )?;
    // Movement is required for a fully current pair (#2922).
    git_commit(&verify, "after movement")?;
    let after = capture(
        ripr,
        &verify,
        &["check", "--root", ".", "--format", "repo-exposure-json"],
    )?;
    write(
        &verify.join("target/ripr/pilot/after.repo-exposure.json"),
        &after,
    )?;
    // The gap ledger goes stale if its source sits outside the checkout.
    let after_path = verify.join("after.repo-exposure.json");
    write(&after_path, &after)?;
    let ledger = verify.join("gap-ledger.json");
    run(
        ripr,
        &verify,
        &[
            "reports",
            "gap-ledger",
            "--repo-exposure",
            &after_path.to_string_lossy(),
            "--root",
            ".",
            "--out",
            &ledger.to_string_lossy(),
            "--out-md",
            &verify.join("gap-ledger.md").to_string_lossy(),
        ],
    )?;
    let ledger_value: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&ledger).map_err(|e| format!("read gap ledger: {e}"))?,
    )
    .map_err(|e| format!("parse gap ledger: {e}"))?;
    let gap_id = ledger_value["records"][0]["gap_id"]
        .as_str()
        .ok_or("fixture must emit one gap record")?
        .to_string();
    // verify-execute binds packet and result paths to the repository root.
    let packet = verify.join("packet.json");
    write(
        &packet,
        &capture(
            ripr,
            &verify,
            &[
                "agent",
                "packet",
                "--root",
                ".",
                "--gap-ledger",
                &ledger.to_string_lossy(),
                "--gap-id",
                &gap_id,
                "--json",
            ],
        )?,
    )?;
    let committed = verify.join("result.json");
    run(
        ripr,
        &verify,
        &[
            "agent",
            "verify-execute",
            "--root",
            ".",
            "--packet",
            "packet.json",
            "--result-json",
            "result.json",
            "--authorize",
            "--json",
        ],
    )?;

    let result = out.join("execution-result.json");
    fs::copy(&committed, &result)
        .map_err(|e| format!("verify-execute committed no result: {e}"))?;

    Ok(artifact_specs(
        root, &doctor, &evidence, &review, &attempt, &result,
    ))
}

/// The `--artifact` values, one per row with no committed producer bytes.
fn artifact_specs(
    root: &Path,
    doctor: &Path,
    evidence: &Path,
    review: &Path,
    attempt: &Path,
    result: &Path,
) -> Vec<String> {
    vec![
        format!("schemas/ripr/doctor.schema.json={}", rel(root, doctor)),
        format!(
            "schemas/ripr/pr-evidence.schema.json={}",
            rel(root, evidence)
        ),
        format!(
            "schemas/ripr/review-comments.schema.json={}",
            rel(root, review)
        ),
        format!(
            "schemas/ripr/repair-attempt.schema.json={}",
            rel(root, attempt)
        ),
        format!(
            "schemas/ripr/repair-assurance.schema.json#/$defs/execution_result={}#/result",
            rel(root, result)
        ),
    ]
}

/// The binary's `--version` line is `ripr X (<sha>[-dirty])`. Only an exact,
/// clean match of the replayed commit binds the bytes to the cut.
fn check_identity(version: &str, commit: &str, ripr: &Path, rev: &str) -> Result<(), String> {
    let reported = version
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inside, _)| inside);
    if reported == Some(commit) {
        return Ok(());
    }
    Err(format!(
        "{} reports `{}` but the replay needs a clean build of {commit}; build the binary from that commit \
         (`cargo build -p ripr` at {rev}) or pass --ripr",
        ripr.display(),
        version.trim()
    ))
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Writes the fixture's head-state files from `commit` into a fresh git repo.
fn seed_repo(root: &Path, commit: &str, repo: &Path) -> Result<(), String> {
    for relative in FIXTURE_FILES {
        let bytes = git_blob(root, commit, &format!("{FIXTURE}/input/{relative}"))?;
        let path = repo.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    git(repo, &["init", "-q"])?;
    // Byte-identical checkouts: the edit cage digests raw bytes.
    git(repo, &["config", "core.autocrlf", "false"])?;
    write(&repo.join(".git/info/exclude"), "/target/\n")
}

fn sole_attempt(repo: &Path) -> Result<String, String> {
    let dir = repo.join("target/ripr/repair-attempts");
    let mut ids = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("read {}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // Lock files sit beside the attempt directories.
        if !name.starts_with('.') && entry.path().is_dir() {
            ids.push(name);
        }
    }
    match ids.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(format!(
            "expected exactly one repair attempt, found {ids:?}"
        )),
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn git_commit(repo: &Path, message: &str) -> Result<(), String> {
    git(
        repo,
        &[
            "-c",
            "user.name=RIPR cut replay",
            "-c",
            "user.email=ripr@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            message,
        ],
    )
}

fn git(repo: &Path, args: &[&str]) -> Result<(), String> {
    run(Path::new("git"), repo, args)
}

fn git_text(repo: &Path, args: &[&str]) -> Result<String, String> {
    capture(Path::new("git"), repo, args).map(|text| text.trim().to_string())
}

fn git_blob(root: &Path, commit: &str, path: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["show", &format!("{commit}:{path}")])
        .output()
        .map_err(|e| format!("git show {commit}:{path}: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "git show {commit}:{path}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn run(program: &Path, cwd: &Path, args: &[&str]) -> Result<(), String> {
    capture(program, cwd, args).map(|_| ())
}

/// Runs a command and returns stdout; a nonzero exit is an error that carries
/// stderr, so a producer that starts failing names itself instead of turning
/// into a `not_run` row.
fn capture(program: &Path, cwd: &Path, args: &[&str]) -> Result<String, String> {
    // Scratch repositories must not inherit an outer repository's identity.
    let output = Command::new(program)
        .current_dir(cwd)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args(args)
        .output()
        .map_err(|e| format!("spawn {}: {e}", program.display()))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "`{} {}` in {} exited {:?}: {}",
            program.display(),
            args.join(" "),
            cwd.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_exact_clean_commit_identity_is_accepted() -> Result<(), String> {
        let sha = "a".repeat(40);
        let other = "b".repeat(40);
        let check = |line: &str| check_identity(line, &sha, Path::new("ripr"), "HEAD");
        check(&format!("ripr 0.11.0 ({sha})\n"))?;
        assert!(check(&format!("ripr 0.11.0 ({sha}-dirty)")).is_err());
        assert!(check(&format!("ripr 0.11.0 ({other})")).is_err());
        assert!(check("ripr 0.11.0").is_err());
        let error = check("echo").err().unwrap_or_default();
        assert!(error.contains("clean build"), "{error}");
        Ok(())
    }

    #[test]
    fn the_five_specs_name_distinct_rows_the_sweep_binds() -> Result<(), String> {
        let p = Path::new("x.json");
        let specs = artifact_specs(Path::new("."), p, p, p, p, p);
        assert_eq!(specs.len(), 5);
        let sweep = include_str!("schema_producer_sweep.rs");
        let mut seen = std::collections::BTreeSet::new();
        for spec in &specs {
            let (schema, file) = spec.split_once('=').ok_or("spec has no `=`")?;
            assert!(file.starts_with("x.json"), "{spec}");
            let (path, _) = schema.split_once('#').unwrap_or((schema, ""));
            assert!(
                sweep.contains(&format!("\"{path}\"")),
                "{path} is not bound"
            );
            assert!(seen.insert(schema.to_string()), "{schema} repeated");
        }
        Ok(())
    }
}
