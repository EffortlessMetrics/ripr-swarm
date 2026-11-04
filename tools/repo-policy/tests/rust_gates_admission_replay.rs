//! Executable replay of the `Verify admitted integration before proof` step.
//! The step body is extracted from `rust-gates.yml` and run under Bash in a
//! merge-group-shaped environment: no `pull_request` payload fields, only the
//! caller-supplied constituent inputs. Hosted proof and a real caller are not
//! exercised.
#![cfg(target_os = "linux")]
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const WORKFLOW: &str = include_str!("../../../.github/workflows/rust-gates.yml");
const STEP: &str = "      - name: Verify admitted integration before proof\n";
const GENERATION: &str = "5555555555555555555555555555555555555555";
const REPO: &str = "EffortlessMetrics/ripr-swarm";
const REPO_ID: &str = "R_kgDOTEST";
const PR_ID: &str = "PR_kwDOTEST";
const PR_NUMBER: &str = "11988";
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn step_text() -> TestResult<&'static str> {
    let (_, tail) = WORKFLOW
        .split_once(STEP)
        .ok_or("verification step exists")?;
    Ok(tail.split_once("\n      - ").map_or(tail, |(s, _)| s))
}

fn script() -> TestResult<String> {
    let (_, run) = step_text()?
        .split_once("        run: |\n")
        .ok_or("step has a run block")?;
    Ok(run
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn run(dir: &Path, program: &str, args: &[&str]) -> TestResult<String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()?;
    if !out.status.success() {
        return Err(format!(
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

fn git(dir: &Path, args: &[&str]) -> TestResult<String> {
    let mut all = vec!["-c", "commit.gpgsign=false"];
    all.extend_from_slice(args);
    run(dir, "git", &all)
}

fn canonical_identity(json: &str) -> TestResult<String> {
    let mut child = Command::new("jq")
        .args(["-cS", "."])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("jq stdin")?
        .write_all(json.as_bytes())?;
    let out = child.wait_with_output()?;
    let canonical = String::from_utf8(out.stdout)?;
    let digest = Sha256::digest(canonical.trim_end_matches('\n').as_bytes());
    Ok(digest.iter().fold(String::new(), |mut acc, byte| {
        let _ = write!(acc, "{byte:02x}");
        acc
    }))
}

struct Fixture {
    dir: PathBuf,
    head: String,
    base: String,
    merge: String,
    tree: String,
    swapped_merge: String,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let dir = std::env::temp_dir().join(format!(
            "rust-gates-admission-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir)?;
        git(&dir, &["init", "-q", "-b", "main"])?;
        std::fs::write(dir.join("a"), "a")?;
        git(&dir, &["add", "a"])?;
        git(&dir, &["commit", "-q", "-m", "base"])?;
        let base = git(&dir, &["rev-parse", "HEAD"])?;
        git(&dir, &["checkout", "-q", "-b", "pr"])?;
        std::fs::write(dir.join("b"), "b")?;
        git(&dir, &["add", "b"])?;
        git(&dir, &["commit", "-q", "-m", "head"])?;
        let head = git(&dir, &["rev-parse", "HEAD"])?;
        // Parent order [base, head], as the merge queue builds it.
        git(&dir, &["checkout", "-q", "main"])?;
        git(&dir, &["merge", "-q", "--no-ff", "-m", "merge", "pr"])?;
        let merge = git(&dir, &["rev-parse", "HEAD"])?;
        let tree = git(&dir, &["rev-parse", "HEAD^{tree}"])?;
        // Parent order [head, base]: same tree, wrong order, built with exactly those parents.
        let swapped_merge = git(
            &dir,
            &[
                "commit-tree",
                &tree,
                "-p",
                &head,
                "-p",
                &base,
                "-m",
                "swapped",
            ],
        )?;
        git(&dir, &["checkout", "-q", "--detach", &merge])?;
        Ok(Self {
            dir,
            head,
            base,
            merge,
            tree,
            swapped_merge,
        })
    }

    fn subject(&self) -> serde_json::Value {
        serde_json::json!({
            "version": 2,
            "repository": REPO,
            "repository_id": REPO_ID,
            "pr_id": PR_ID,
            "pr_number": 11988,
            "destination_repository_id": REPO_ID,
            "destination_ref": "refs/heads/main",
            "head": self.head,
            "base": self.base,
            "merge": self.merge,
            "tree": self.tree,
            "parents": [self.base, self.head],
            "workflow_generation": GENERATION,
        })
    }

    /// A fully valid qualification environment for the checked-out merge commit.
    fn env(&self) -> TestResult<BTreeMap<&'static str, String>> {
        self.env_for(&self.subject())
    }

    fn env_for(&self, subject: &serde_json::Value) -> TestResult<BTreeMap<&'static str, String>> {
        let json = subject.to_string();
        let mut env = BTreeMap::new();
        for (key, value) in [
            ("PROFILE", "qualification"),
            ("ADMITTED_HEAD", self.head.as_str()),
            ("ADMITTED_BASE", self.base.as_str()),
            ("ADMITTED_MERGE", self.merge.as_str()),
            ("ADMITTED_TREE", self.tree.as_str()),
            ("GENERATION", GENERATION),
            ("GITHUB_WORKFLOW_SHA", GENERATION),
            ("GITHUB_REPOSITORY", REPO),
            ("REPOSITORY_ID", REPO_ID),
            ("PR_ID", PR_ID),
            ("PR_NUMBER", PR_NUMBER),
            ("CONSTITUENT_HEAD", self.head.as_str()),
        ] {
            env.insert(key, value.to_string());
        }
        env.insert("IDENTITY", canonical_identity(&json)?);
        env.insert("SUBJECT_JSON", json);
        Ok(env)
    }

    fn replay(&self, env: &BTreeMap<&'static str, String>) -> TestResult<(Output, String)> {
        let out_file = self.dir.join(format!(
            "github-output-{}",
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::write(&out_file, "")?;
        let mut cmd = Command::new("bash");
        cmd.args(["-e", "-c", &script()?])
            .current_dir(&self.dir)
            .env_clear()
            .env("PATH", std::env::var("PATH")?)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GITHUB_OUTPUT", &out_file);
        // Merge-group shaped: no pull_request payload fields are ever provided.
        for (key, value) in env {
            cmd.env(key, value);
        }
        let output = cmd.output()?;
        Ok((output, std::fs::read_to_string(&out_file)?))
    }

    fn rejects(&self, label: &str, env: &BTreeMap<&'static str, String>) -> TestResult {
        let (out, published) = self.replay(env)?;
        assert!(
            !out.status.success(),
            "{label}: expected fail-closed, got success; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            published.is_empty(),
            "{label}: published outputs {published:?}"
        );
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn verification_step_never_reads_the_pull_request_payload() -> TestResult {
    let step = step_text()?;
    assert!(!step.contains("github.event.pull_request"), "{step}");
    for (var, input) in [
        ("PR_ID", "constituent-pr-node-id"),
        ("PR_NUMBER", "constituent-pr-number"),
        ("CONSTITUENT_HEAD", "constituent-head-sha"),
    ] {
        let line = format!("          {var}: ${{{{ inputs.{input} }}}}\n");
        assert!(step.contains(&line), "{var} must map to inputs.{input}");
    }
    Ok(())
}

#[test]
fn valid_merge_group_shaped_tuple_passes_and_publishes_identities() -> TestResult {
    let f = Fixture::new()?;
    let (out, published) = f.replay(&f.env()?)?;
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected = format!(
        "head={}\nbase={}\nmerge={}\ntree={}\n",
        f.head, f.base, f.merge, f.tree
    );
    assert_eq!(published, expected);
    Ok(())
}

#[test]
fn constituent_identity_missing_empty_or_contradictory_fails_closed() -> TestResult {
    let f = Fixture::new()?;
    let six = "6".repeat(40);
    for (label, key, value) in [
        ("empty PR number", "PR_NUMBER", ""),
        ("zero PR number", "PR_NUMBER", "0"),
        ("non-numeric PR number", "PR_NUMBER", "1e3"),
        ("wrong PR number", "PR_NUMBER", "11989"),
        ("empty PR node id", "PR_ID", ""),
        ("wrong PR node id", "PR_ID", "PR_kwDOOTHER"),
        ("empty constituent head", "CONSTITUENT_HEAD", ""),
        ("wrong constituent head", "CONSTITUENT_HEAD", six.as_str()),
        ("empty repository id", "REPOSITORY_ID", ""),
        ("wrong repository id", "REPOSITORY_ID", "R_kgDOOTHER"),
    ] {
        let mut env = f.env()?;
        env.insert(key, value.to_string());
        f.rejects(label, &env)?;
    }
    for key in ["PR_NUMBER", "PR_ID", "CONSTITUENT_HEAD", "REPOSITORY_ID"] {
        let mut env = f.env()?;
        env.remove(key);
        f.rejects(&format!("absent {key}"), &env)?;
    }
    Ok(())
}

#[test]
fn stale_or_wrong_head_base_tree_and_merge_fail_closed() -> TestResult {
    let f = Fixture::new()?;
    let other = "7".repeat(40);
    for (label, key) in [
        ("wrong head", "ADMITTED_HEAD"),
        ("wrong base", "ADMITTED_BASE"),
        ("wrong merge", "ADMITTED_MERGE"),
        ("wrong tree", "ADMITTED_TREE"),
        ("stale generation", "GENERATION"),
    ] {
        let mut env = f.env()?;
        env.insert(key, other.clone());
        f.rejects(label, &env)?;
    }
    let mut env = f.env()?;
    env.insert("GITHUB_WORKFLOW_SHA", other.clone());
    f.rejects("generation differs from running workflow", &env)?;
    // Agreeing subject JSON and inputs cannot rescue a checkout that is not the subject.
    for (label, key) in [("tree", "tree"), ("head", "head")] {
        let mut subject = f.subject();
        subject[key] = other.clone().into();
        let mut env = f.env_for(&subject)?;
        env.insert(
            if key == "tree" {
                "ADMITTED_TREE"
            } else {
                "ADMITTED_HEAD"
            },
            other.clone(),
        );
        env.insert("CONSTITUENT_HEAD", other.clone());
        f.rejects(&format!("self-consistent wrong {label}"), &env)?;
    }
    Ok(())
}

#[test]
fn parent_order_mismatch_fails_closed() -> TestResult {
    let f = Fixture::new()?;
    let mut subject = f.subject();
    subject["parents"] = serde_json::json!([f.head, f.base]);
    f.rejects("subject JSON parents reversed", &f.env_for(&subject)?)?;
    // A merge whose real parents are [head, base] fails the checked-out parent check
    // even when JSON and inputs all claim [base, head].
    git(&f.dir, &["checkout", "-q", "--detach", &f.swapped_merge])?;
    let mut subject = f.subject();
    subject["merge"] = f.swapped_merge.clone().into();
    let mut env = f.env_for(&subject)?;
    env.insert("ADMITTED_MERGE", f.swapped_merge.clone());
    f.rejects("checked-out parents reversed", &env)?;
    Ok(())
}

#[test]
fn malformed_or_mismatched_subject_json_fails_closed() -> TestResult {
    let f = Fixture::new()?;
    for (label, json) in [
        ("empty", String::new()),
        ("not json", "{not json".to_string()),
        ("truncated", f.subject().to_string()[..40].to_string()),
        ("array", "[]".to_string()),
    ] {
        let mut env = f.env()?;
        env.insert("IDENTITY", canonical_identity(&json).unwrap_or_default());
        env.insert("SUBJECT_JSON", json);
        f.rejects(label, &env)?;
    }
    let mut env = f.env()?;
    env.insert("IDENTITY", "8".repeat(64));
    f.rejects("identity digest mismatch", &env)?;
    let mut env = f.env()?;
    env.insert("IDENTITY", "short".to_string());
    f.rejects("malformed identity", &env)?;
    for (label, key, value) in [
        ("version", "version", serde_json::json!(1)),
        (
            "destination ref",
            "destination_ref",
            "refs/heads/other".into(),
        ),
        ("repository", "repository", "other/repo".into()),
        ("pr number", "pr_number", 11989.into()),
        ("pr id", "pr_id", "PR_other".into()),
    ] {
        let mut subject = f.subject();
        subject[key] = value;
        f.rejects(label, &f.env_for(&subject)?)?;
    }
    Ok(())
}

#[test]
fn legacy_profile_is_inert_and_rejects_partial_or_unknown_input() -> TestResult {
    let f = Fixture::new()?;
    let mut legacy = BTreeMap::new();
    legacy.insert("PROFILE", "legacy".to_string());
    for key in [
        "SUBJECT_JSON",
        "ADMITTED_HEAD",
        "ADMITTED_BASE",
        "ADMITTED_MERGE",
        "ADMITTED_TREE",
        "GENERATION",
        "IDENTITY",
        "PR_ID",
        "PR_NUMBER",
        "CONSTITUENT_HEAD",
    ] {
        legacy.insert(key, String::new());
    }
    let (out, published) = f.replay(&legacy)?;
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(published.is_empty(), "legacy published {published:?}");
    for key in [
        "SUBJECT_JSON",
        "ADMITTED_HEAD",
        "ADMITTED_TREE",
        "IDENTITY",
        "PR_ID",
        "PR_NUMBER",
        "CONSTITUENT_HEAD",
    ] {
        let mut partial = legacy.clone();
        partial.insert(key, "x".to_string());
        f.rejects(&format!("legacy with {key}"), &partial)?;
    }
    let mut unknown = f.env()?;
    unknown.insert("PROFILE", "other".to_string());
    f.rejects("unknown profile", &unknown)?;
    Ok(())
}
