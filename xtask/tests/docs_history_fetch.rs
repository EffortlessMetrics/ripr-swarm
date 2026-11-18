//! Exercise the real docs comparison scripts using Git's bare, blobless and
//! full-checkout paths. No Actions events, credentials or network are involved.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const WORKFLOW: &str = include_str!("../../.github/workflows/routed-rust.yml");
const PUBLIC_URL: &str = "https://github.com/EffortlessMetrics/ripr-swarm.git";
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let base = std::env::temp_dir();
        let dir = base.join(format!(
            "ripr-docs-history-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).map_err(|error| error.to_string())?;
        eprintln!("docs history fixture: {}", dir.display());
        Ok(Self(dir))
    }

    // On an assertion or a returned error, retain the fixture for diagnosis.
    fn clean_after_pass(self) -> Result<(), String> {
        fs::remove_dir_all(&self.0).map_err(|error| error.to_string())
    }
}

fn command(dir: &Path, program: &str) -> Command {
    let mut value = Command::new(program);
    value
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
    value
}

fn output(value: Result<Output, std::io::Error>) -> Result<String, String> {
    let value = value.map_err(|error| error.to_string())?;
    if !value.status.success() {
        return Err(String::from_utf8_lossy(&value.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&value.stdout).trim().to_string())
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    output(command(dir, "git").args(args).output())
}

fn docs_job() -> Result<&'static str, String> {
    let (_, job) = WORKFLOW
        .split_once("  detect-docs-only:\n")
        .ok_or("missing docs detector")?;
    let (job, _) = job
        .split_once("\n  admission:\n")
        .ok_or("missing next workflow job")?;
    Ok(job)
}

fn actual_step(name: &str) -> Result<String, String> {
    let marker = format!("      - name: {name}\n");
    let (_, step) = docs_job()?
        .split_once(&marker)
        .ok_or_else(|| format!("missing actual workflow step: {name}"))?;
    let (_, body) = step
        .split_once("        run: |\n")
        .ok_or_else(|| format!("missing run body: {name}"))?;
    Ok(body
        .lines()
        .map_while(|line| line.strip_prefix("          "))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn classify(dir: &Path, subject: &Subject<'_>, bare: &str) -> Result<String, String> {
    let result = subject.tmp.join("scope-output");
    let summary = subject.tmp.join("scope-summary");
    fs::write(&result, "").map_err(|error| error.to_string())?;
    output(
        command(dir, "bash")
            .arg("-e")
            .arg("-c")
            .arg(actual_step(
                "Check whether every changed file is docs/spec/markdown",
            )?)
            .env("EVENT_NAME", subject.event)
            .env("BASE_SHA", subject.base)
            .env("HEAD_SHA", subject.head)
            .env("DOCS_GIT_DIR", bare)
            .env("GITHUB_OUTPUT", &result)
            .env("GITHUB_STEP_SUMMARY", &summary)
            .output(),
    )?;
    fs::read_to_string(result).map_err(|error| error.to_string())
}

struct Subject<'a> {
    tmp: &'a Path,
    event: &'a str,
    base: &'a str,
    head: &'a str,
}

fn acquire(subject: &Subject<'_>, source: &Path) -> Result<String, String> {
    let result = subject.tmp.join("git-output");
    fs::write(&result, "").map_err(|error| error.to_string())?;
    output(
        command(subject.tmp, "bash")
            .arg("-c")
            .arg(actual_step(
                "Fetch exact docs comparison history without blobs",
            )?)
            .env("SERVER_URL", "https://github.com")
            .env("REPOSITORY", "EffortlessMetrics/ripr-swarm")
            .env("BASE_SHA", subject.base)
            .env("HEAD_SHA", subject.head)
            .env("RUNNER_TEMP", subject.tmp)
            .env("GITHUB_OUTPUT", &result)
            .env("GIT_CONFIG_COUNT", "1")
            .env(
                "GIT_CONFIG_KEY_0",
                format!("url.file://{}.insteadOf", source.display()),
            )
            .env("GIT_CONFIG_VALUE_0", PUBLIC_URL)
            .output(),
    )?;
    fs::read_to_string(result).map_err(|error| error.to_string())
}

fn commit(dir: &Path, path: &str, contents: &str) -> Result<String, String> {
    fs::write(dir.join(path), contents).map_err(|error| error.to_string())?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", path])?;
    git(dir, &["rev-parse", "HEAD"])
}

#[test]
fn exact_blobless_and_full_paths_classify_the_same_real_git_histories() -> Result<(), String> {
    let scratch = Scratch::new()?;
    let origin = scratch.0.join("origin");
    fs::create_dir(&origin).map_err(|error| error.to_string())?;
    git(&origin, &["init", "-q"])?;
    git(&origin, &["config", "uploadpack.allowFilter", "true"])?;
    git(
        &origin,
        &["config", "uploadpack.allowAnySHA1InWant", "true"],
    )?;
    let base = commit(&origin, "old.txt", "payload\n")?;
    let docs = commit(&origin, "with spaces\nand newline.md", "# Doc\n")?;
    git(&origin, &["mv", "old.txt", "renamed.md"])?;
    let non_docs = commit(&origin, "more.md", "# More\n")?;
    git(&origin, &["checkout", "--detach", &base])?;
    let advanced = commit(&origin, "base-side.rs", "fn base_only() {}\n")?;
    git(&origin, &["checkout", "--detach", &non_docs])?;

    for event in ["pull_request", "merge_group"] {
        for (base, head, expected) in [
            (base.as_str(), docs.as_str(), "docs_only=true\n"),
            (base.as_str(), non_docs.as_str(), "docs_only=false\n"),
            (base.as_str(), base.as_str(), "docs_only=false\n"),
            // The two-dot diff would see the unrelated base-side Rust file.
            (advanced.as_str(), docs.as_str(), "docs_only=true\n"),
        ] {
            let subject = Subject {
                tmp: &scratch.0,
                event,
                base,
                head,
            };
            let acquired = acquire(&subject, &origin)?;
            let bare = acquired
                .trim()
                .strip_prefix("git_dir=")
                .ok_or("exact history was not acquired")?;
            assert_eq!(
                git(Path::new(bare), &["rev-parse", "--is-shallow-repository"])?,
                "false"
            );
            assert_eq!(classify(&scratch.0, &subject, bare)?, expected);
            assert_eq!(classify(&origin, &subject, "")?, expected);
        }
    }

    let subject = Subject {
        tmp: &scratch.0,
        event: "merge_group",
        base: &base,
        head: &docs,
    };
    // Simulate a fetch miss before classifying with the original full checkout.
    assert_eq!(acquire(&subject, &scratch.0.join("missing-origin"))?, "");
    assert_eq!(classify(&origin, &subject, "")?, "docs_only=true\n");

    // With no authoritative history at all, classification never becomes true.
    let absent = Subject {
        head: "0000000000000000000000000000000000000001",
        ..subject
    };
    assert_eq!(acquire(&absent, &origin)?, "");
    assert_eq!(classify(&origin, &absent, "")?, "docs_only=false\n");

    let shallow = scratch.0.join("shallow");
    let url = format!("file://{}", origin.display());
    git(
        &scratch.0,
        &["clone", "-q", "--depth", "1", &url, "shallow"],
    )?;
    let shallow_subject = Subject {
        head: &non_docs,
        ..absent
    };
    assert_eq!(
        git(&shallow, &["rev-parse", "--is-shallow-repository"])?,
        "true"
    );
    assert_eq!(
        classify(&shallow, &shallow_subject, "")?,
        "docs_only=false\n"
    );
    scratch.clean_after_pass()
}

#[test]
fn fallback_and_event_scope_are_wired_in_the_actual_job() -> Result<(), String> {
    let job = docs_job()?;
    assert!(job.contains(
        "if: (github.event_name == 'pull_request' || github.event_name == 'merge_group') && !steps.docs_history.outputs.git_dir"
    ));
    assert!(job.contains("fetch-depth: 0"));
    assert!(job.contains("DOCS_GIT_DIR: ${{ steps.docs_history.outputs.git_dir }}"));
    assert!(!job.contains("GITHUB_TOKEN:"));
    assert!(!job.contains("GH_TOKEN:"));
    Ok(())
}
