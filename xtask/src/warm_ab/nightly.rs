//! The nightly orchestration for `warm-ab --pins` (#10639): resolve the two
//! builds, build both release binaries, make one pinned tree and diff per pin,
//! and run the A/B for each. It lives here rather than in the workflow so the
//! workflow stays one command and the logic is tested.

use std::fs;
use std::path::{Path, PathBuf};

use crate::run as shared_run;

use super::{Options, report};

/// How far back the default baseline is taken from the candidate.
const BASELINE_AGE: &str = "24 hours ago";

/// Where the scoreboard declares the external repositories and their pins.
const SCOREBOARD_CONFIG: &str = "benchmarks/dx_scoreboard/scoreboards.toml";

/// One pinned change of an external repository, read from the scoreboard
/// corpus so the A/B times the exact diff the scoreboard row measures.
pub(crate) struct ExternalPin {
    id: String,
    url: String,
    sha: String,
    base: String,
}

pub(crate) struct Plan {
    /// `(id, commit)` of each pinned diff of this repository.
    pins: Vec<(String, String)>,
    /// Scoreboard corpus ids whose pinned change is timed as well (#11876).
    external_ids: Vec<String>,
    baseline_ref: Option<String>,
    candidate_ref: Option<String>,
}

impl Plan {
    pub(crate) fn new(
        pins: Option<&str>,
        external: Option<&str>,
        baseline_ref: Option<String>,
        candidate_ref: Option<String>,
    ) -> Result<Self, String> {
        let blank = |value: Option<String>| value.filter(|text| !text.trim().is_empty());
        let external_ids = match external {
            Some(text) => parse_external_ids(text)?,
            None => Vec::new(),
        };
        let pins = match pins {
            Some(text) => parse_pins(text)?,
            None if external_ids.is_empty() => {
                return Err("--pins or --external-pins needs at least one entry".to_string());
            }
            None => Vec::new(),
        };
        Ok(Self {
            pins,
            external_ids,
            baseline_ref: blank(baseline_ref),
            candidate_ref: blank(candidate_ref),
        })
    }
}

/// `id=commit,id=commit`. An id names a report file, so it is restricted; a
/// commit is only ever passed to git as a revision, never through a shell.
pub(crate) fn parse_pins(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut pins = Vec::new();
    for entry in text.split(',').filter(|entry| !entry.is_empty()) {
        let (id, commit) = entry
            .split_once('=')
            .ok_or_else(|| format!("pin `{entry}` must look like id=commit"))?;
        let id_ok = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        let commit_ok = !commit.is_empty()
            && !commit.starts_with('-')
            && commit.chars().all(|c| c.is_ascii_alphanumeric());
        if !id_ok || !commit_ok {
            return Err(format!(
                "pin `{entry}` needs an id of letters, digits, `-` or `_` and a commit of letters and digits"
            ));
        }
        if pins.iter().any(|(seen, _): &(String, String)| seen == id) {
            return Err(format!("pin id `{id}` is repeated"));
        }
        pins.push((id.to_string(), commit.to_string()));
    }
    if pins.is_empty() {
        return Err("--pins needs at least one id=commit".to_string());
    }
    Ok(pins)
}

/// `id,id` naming scoreboard corpus entries. An id names a report file and a
/// directory, so it is restricted the same way a pin id is.
pub(crate) fn parse_external_ids(text: &str) -> Result<Vec<String>, String> {
    let mut ids: Vec<String> = Vec::new();
    for id in text.split(',').filter(|id| !id.is_empty()) {
        let id_ok = id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !id_ok {
            return Err(format!(
                "external pin `{id}` needs letters, digits, `-` or `_`"
            ));
        }
        if ids.iter().any(|seen| seen == id) {
            return Err(format!("external pin `{id}` is repeated"));
        }
        ids.push(id.to_string());
    }
    if ids.is_empty() {
        return Err("--external-pins needs at least one corpus id".to_string());
    }
    Ok(ids)
}

/// Resolve each id against the scoreboard corpus. An id the corpus does not
/// declare is an error, never a silent skip: a gate that quietly times
/// nothing would pass for the wrong reason.
pub(crate) fn resolve_external(
    ids: &[String],
    corpus: &[crate::reports::dx_scoreboard::CorpusEntry],
) -> Result<Vec<ExternalPin>, String> {
    ids.iter()
        .map(|id| {
            let entry = corpus
                .iter()
                .find(|entry| entry.id == *id)
                .ok_or_else(|| format!("external pin `{id}` is not in the scoreboard corpus"))?;
            Ok(ExternalPin {
                id: entry.id.clone(),
                url: entry.url.clone(),
                sha: entry.sha.clone(),
                base: entry
                    .base_sha
                    .clone()
                    .unwrap_or_else(|| format!("{}^", entry.sha)),
            })
        })
        .collect()
}

fn git(args: &[&str], cwd: &Path) -> Result<String, String> {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let output =
        shared_run::capture_output_in_dir("git", &owned, cwd, &format!("git {}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            output.stderr.trim()
        ));
    }
    Ok(output.stdout.trim().to_string())
}

/// Candidate: the given ref or HEAD. Baseline: the given ref or the newest
/// first-parent commit at least a day older than the candidate.
fn resolve(plan: &Plan, repo: &Path) -> Result<(String, String), String> {
    let candidate = git(
        &[
            "rev-parse",
            "--verify",
            plan.candidate_ref.as_deref().unwrap_or("HEAD"),
        ],
        repo,
    )?;
    let baseline = match &plan.baseline_ref {
        Some(reference) => git(&["rev-parse", "--verify", reference], repo)?,
        None => {
            let before = format!("--before={BASELINE_AGE}");
            let found = git(
                &["rev-list", "-1", "--first-parent", &before, &candidate],
                repo,
            )?;
            if found.is_empty() {
                return Err(format!(
                    "no commit older than {BASELINE_AGE} to use as the baseline"
                ));
            }
            found
        }
    };
    Ok((baseline, candidate))
}

fn worktree(repo: &Path, path: &Path, commit: &str) -> Result<(), String> {
    git(
        &[
            "worktree",
            "add",
            "--detach",
            &path.to_string_lossy(),
            commit,
        ],
        repo,
    )
    .map(|_| ())
}

fn build(source: &Path, name: &str, bins: &Path) -> Result<PathBuf, String> {
    let status = shared_run::run_in_dir(
        Path::new("cargo"),
        &["build", "--release", "--locked", "-p", "ripr"],
        source,
    )?;
    if !status.success() {
        return Err(format!("the {name} release build failed"));
    }
    let built = source.join("target/release/ripr");
    let kept = bins.join(format!("ripr-{name}"));
    fs::copy(&built, &kept)
        .map_err(|error| format!("failed to copy {}: {error}", built.display()))?;
    Ok(kept)
}

pub(crate) fn run(options: &Options, plan: &Plan) -> Result<(), String> {
    let repo = std::env::current_dir()
        .map_err(|error| format!("failed to read the working directory: {error}"))?;
    let (baseline, candidate) = resolve(plan, &repo)?;
    println!("warm-ab: baseline {baseline}, candidate {candidate}");
    let scratch = repo
        .join(&options.work)
        .join(format!("nightly-{}", std::process::id()));
    if scratch.exists() {
        return Err(format!(
            "owned work root {} already exists; refusing to reuse it",
            scratch.display()
        ));
    }
    fs::create_dir_all(&scratch)
        .map_err(|error| format!("failed to create {}: {error}", scratch.display()))?;
    let result = run_in(options, plan, &repo, &scratch, (&baseline, &candidate));
    // Best effort: the runner is discarded, but a local run should not leave
    // registered worktrees behind.
    if let Ok(entries) = fs::read_dir(&scratch) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join(".git").exists() {
                let _ = git(
                    &["worktree", "remove", "--force", &path.to_string_lossy()],
                    &repo,
                );
            }
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    result
}

/// Clone one external pin, check out its commit and write the pinned diff.
fn prepare_external(external: &ExternalPin, scratch: &Path) -> Result<(PathBuf, PathBuf), String> {
    let tree = scratch.join(format!("ext-{}", external.id));
    let tree_text = tree.to_string_lossy().into_owned();
    git(
        &[
            "clone",
            "--quiet",
            "--filter=blob:none",
            &external.url,
            &tree_text,
        ],
        scratch,
    )?;
    git(&["checkout", "--quiet", "--detach", &external.sha], &tree)?;
    let diff = git(&["diff", &external.base, &external.sha], &tree)?;
    let diff_path = scratch.join(format!("ext-{}.diff", external.id));
    fs::write(&diff_path, diff + "\n")
        .map_err(|error| format!("failed to write {}: {error}", diff_path.display()))?;
    Ok((tree, diff_path))
}

fn run_in(
    options: &Options,
    plan: &Plan,
    repo: &Path,
    scratch: &Path,
    (baseline, candidate): (&str, &str),
) -> Result<(), String> {
    let bins = scratch.join("bin");
    fs::create_dir_all(&bins)
        .map_err(|error| format!("failed to create {}: {error}", bins.display()))?;
    let mut built = Vec::new();
    for (name, commit) in [("baseline", baseline), ("candidate", candidate)] {
        let source = scratch.join(format!("{name}-src"));
        worktree(repo, &source, commit)?;
        built.push(build(&source, name, &bins)?);
    }
    let mut failed = Vec::new();
    let pair = |id: &str, tree: PathBuf, diff: PathBuf| Options {
        baseline: built[0].clone(),
        candidate: built[1].clone(),
        tree,
        diff,
        pin: id.to_string(),
        runs: options.runs,
        out: options.out.clone(),
        work: options.work.clone(),
        limits: options.limits,
        nightly: None,
    };
    for (id, commit) in &plan.pins {
        let tree = scratch.join(format!("pin-{id}"));
        worktree(repo, &tree, commit)?;
        let parent = format!("{commit}^");
        let diff = git(&["diff", &parent, commit], repo)?;
        let diff_path = scratch.join(format!("pin-{id}.diff"));
        fs::write(&diff_path, diff + "\n")
            .map_err(|error| format!("failed to write {}: {error}", diff_path.display()))?;
        if let Err(error) = report(&pair(id, tree, diff_path)) {
            eprintln!("{error}");
            failed.push(id.clone());
        }
    }
    if !plan.external_ids.is_empty() {
        let corpus = crate::reports::dx_scoreboard::load_config(Path::new(SCOREBOARD_CONFIG))?;
        for external in resolve_external(&plan.external_ids, &corpus.corpus)? {
            // A clone, checkout or diff that fails costs this pin its verdict,
            // not the pins after it.
            let prepared = prepare_external(&external, scratch);
            let outcome =
                prepared.and_then(|(tree, diff_path)| report(&pair(&external.id, tree, diff_path)));
            if let Err(error) = outcome {
                eprintln!("{error}");
                failed.push(external.id.clone());
            }
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("warm-ab failed for: {}", failed.join(", ")))
    }
}
