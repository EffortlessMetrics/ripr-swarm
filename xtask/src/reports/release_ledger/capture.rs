//! Local-Git capture for the direct-main ledger: the exact `base..cut` range,
//! per-commit touched paths, and the final-tree effect of every commit.
//!
//! Only Git is read. Final tree effect is derived exactly and cheaply first
//! (the last first-parent toucher of a path alive at the cut owns that path's
//! final blob), and `git blame` runs only for alive paths with a toucher that
//! is still undecided. A blame failure fails the capture: an unknown effect is
//! never defaulted to a value.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use super::dto::{
    EffectBasis, HISTORY_NO_BLOB_EFFECT, MergedPr, PRESENT_BUT_SUPERSEDED, PRESENT_EFFECTIVE,
    Range, SCHEMA_VERSION, Source,
};
use crate::reports::release_denominator::is_sha;

pub(super) const FACTS_KIND: &str = "release_ledger_git_facts";
const GIT_TIMEOUT: Duration = Duration::from_mins(5);
const BLAME_TIMEOUT: Duration = Duration::from_mins(2);
const SHA_CHUNK: usize = 200;

pub(super) struct CaptureArgs<'a> {
    pub(super) repo: &'a Path,
    pub(super) repository: &'a str,
    pub(super) base: &'a str,
    pub(super) cut: &'a str,
    pub(super) main: &'a str,
}

/// Everything the rule layer needs, derived from Git alone.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GitFacts {
    pub(super) schema_version: String,
    pub(super) kind: String,
    pub(super) source: Source,
    pub(super) range: Range,
    pub(super) merged_prs: Vec<MergedPr>,
    pub(super) alive_paths_at_cut: u64,
    pub(super) blamed_paths: u64,
    /// Same order as `range.reachable`.
    pub(super) commits: Vec<CommitFacts>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommitFacts {
    pub(super) commit_sha: String,
    pub(super) parent_shas: Vec<String>,
    pub(super) tree_sha: String,
    pub(super) first_parent_position: Option<u64>,
    pub(super) subject: String,
    /// The trailing `(#N)` of a first-parent subject: the protected squash PR.
    pub(super) pr_refs: Vec<u64>,
    /// Every other `#N` named in the subject. Git cannot tell an issue from a
    /// PR, so these are mentions, not verified issue references.
    pub(super) issue_refs: Vec<u64>,
    /// Sorted, unique paths this commit changed against its first parent.
    pub(super) paths: Vec<String>,
    pub(super) final_tree_effect: String,
    pub(super) effect_basis: EffectBasis,
}

struct Commit {
    parents: Vec<String>,
    tree: String,
    subject: String,
}

pub(super) fn capture(args: &CaptureArgs<'_>) -> Result<GitFacts, String> {
    let base = rev_parse(args.repo, args.base)?;
    let cut = rev_parse(args.repo, args.cut)?;
    let main = rev_parse(args.repo, args.main)?;
    let cut_tree = rev_parse(args.repo, &format!("{cut}^{{tree}}"))?;
    let span = format!("{base}..{cut}");

    let first_parent = shas(&git_text(
        args.repo,
        &["rev-list", "--first-parent", "--reverse", &span],
    )?);
    let rest = non_first_parent(
        &first_parent,
        shas(&git_text(args.repo, &["rev-list", &span])?),
    );
    let mut reachable = first_parent.clone();
    reachable.extend(rest);

    let commits = read_commits(args.repo, &span)?;
    let touched = read_touched_paths(args.repo, &reachable)?;
    let alive = alive_paths(args.repo, &cut)?;
    let position: BTreeMap<&String, u64> = first_parent
        .iter()
        .enumerate()
        .map(|(index, sha)| (sha, index as u64 + 1))
        .collect();

    let renamed = read_rename_destinations(args.repo, &reachable)?;
    let owned = derive_ownership(
        args.repo,
        &cut,
        &Ranges {
            first_parent: &first_parent,
            reachable: &reachable,
        },
        &touched,
        &alive,
        &renamed,
    )?;
    let mut records = Vec::with_capacity(reachable.len());
    let mut merged_prs = Vec::new();
    for sha in &reachable {
        let commit = commits
            .get(sha)
            .ok_or_else(|| format!("git log did not return range commit {sha}"))?;
        let paths = touched.get(sha).cloned().unwrap_or_default();
        let basis = effect_basis(sha, &paths, &alive, &owned);
        let (pr_refs, issue_refs) = split_refs(&commit.subject, position.contains_key(sha));
        if let Some(number) = pr_refs.last() {
            merged_prs.push(MergedPr {
                number: *number,
                merge_sha: sha.clone(),
                state: "merged".to_string(),
            });
        }
        records.push(CommitFacts {
            commit_sha: sha.clone(),
            parent_shas: commit.parents.clone(),
            tree_sha: commit.tree.clone(),
            first_parent_position: position.get(sha).copied(),
            subject: commit.subject.clone(),
            pr_refs,
            issue_refs,
            paths: paths.into_iter().collect(),
            final_tree_effect: classify_effect(&basis).to_string(),
            effect_basis: basis,
        });
    }
    Ok(GitFacts {
        schema_version: SCHEMA_VERSION.to_string(),
        kind: FACTS_KIND.to_string(),
        source: Source {
            repository: args.repository.to_string(),
            base_sha: base,
            cut_sha: cut,
            cut_tree_sha: cut_tree,
            main_sha: main,
        },
        range: Range {
            first_parent,
            reachable,
        },
        merged_prs,
        alive_paths_at_cut: alive.len() as u64,
        blamed_paths: owned.blamed_paths,
        commits: records,
    })
}

fn classify_effect(basis: &EffectBasis) -> &'static str {
    if basis.last_toucher_paths + basis.blamed_lines > 0 {
        PRESENT_EFFECTIVE
    } else if basis.alive_paths > 0 {
        PRESENT_BUT_SUPERSEDED
    } else {
        HISTORY_NO_BLOB_EFFECT
    }
}

fn effect_basis(
    sha: &str,
    paths: &BTreeSet<String>,
    alive: &BTreeSet<String>,
    owned: &Ownership,
) -> EffectBasis {
    let alive_paths = paths.iter().filter(|path| alive.contains(*path)).count() as u64;
    EffectBasis {
        alive_paths,
        absent_paths: paths.len() as u64 - alive_paths,
        last_toucher_paths: owned.last_toucher_paths.get(sha).copied().unwrap_or(0),
        blamed_lines: owned.blamed_lines.get(sha).copied().unwrap_or(0),
    }
}

/// Reachable commits that are not first-parent, sorted by SHA.
fn non_first_parent(first_parent: &[String], reachable: Vec<String>) -> Vec<String> {
    let first: BTreeSet<&String> = first_parent.iter().collect();
    let mut rest: Vec<String> = reachable
        .into_iter()
        .filter(|sha| !first.contains(sha))
        .collect();
    rest.sort();
    rest
}

/// Splits `#N` mentions in a subject. On a first-parent commit the trailing
/// `(#N)` is the squash PR; every other mention is a plain reference.
pub(super) fn split_refs(subject: &str, first_parent: bool) -> (Vec<u64>, Vec<u64>) {
    let mut numbers = Vec::new();
    let bytes = subject.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            let digits: String = subject[index + 1..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(number) = digits.parse::<u64>() {
                numbers.push((number, index, index + 1 + digits.len()));
            }
        }
        index += 1;
    }
    let trailing = numbers.last().copied().filter(|(_, start, end)| {
        first_parent && *start > 0 && bytes[*start - 1] == b'(' && subject[*end..].trim_end() == ")"
    });
    let pr_refs: Vec<u64> = trailing.map(|(number, _, _)| number).into_iter().collect();
    let issue_refs = numbers
        .iter()
        .enumerate()
        .filter(|(position, _)| !(trailing.is_some() && *position + 1 == numbers.len()))
        .map(|(_, (number, _, _))| *number)
        .collect();
    (pr_refs, issue_refs)
}

struct Ownership {
    last_toucher_paths: BTreeMap<String, u64>,
    blamed_lines: BTreeMap<String, u64>,
    blamed_paths: u64,
}

struct Ranges<'a> {
    first_parent: &'a [String],
    reachable: &'a [String],
}

fn derive_ownership(
    repo: &Path,
    cut: &str,
    ranges: &Ranges<'_>,
    touched: &BTreeMap<String, BTreeSet<String>>,
    alive: &BTreeSet<String>,
    renamed: &BTreeSet<String>,
) -> Result<Ownership, String> {
    let (first_parent, reachable) = (ranges.first_parent, ranges.reachable);
    // Ordered first-parent touchers per alive path; the last one wrote the
    // path's final blob.
    let mut touchers: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
    for sha in first_parent {
        for path in touched.get(sha).into_iter().flatten() {
            if alive.contains(path) {
                touchers.entry(path).or_default().push(sha);
            }
        }
    }
    let mut last_toucher_paths: BTreeMap<String, u64> = BTreeMap::new();
    let mut decided: BTreeSet<&String> = BTreeSet::new();
    for chain in touchers.values() {
        if let Some(last) = chain.last() {
            *last_toucher_paths.entry((*last).clone()).or_default() += 1;
            decided.insert(last);
        }
    }
    // A path needs blame when any commit touching it, first-parent or not, is
    // not yet known to own something. Paths with a single toucher are settled.
    let mut all_touchers: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
    for sha in reachable {
        for path in touched.get(sha).into_iter().flatten() {
            if alive.contains(path) {
                all_touchers.entry(path).or_default().push(sha);
            }
        }
    }
    let in_range: BTreeSet<&str> = reachable.iter().map(String::as_str).collect();
    // A path another file was renamed into can carry lines written under its
    // old name, which no touched-path set names, so it is always blamed.
    let mut blame_paths: BTreeSet<&String> = all_touchers
        .iter()
        .filter(|(_, chain)| chain.len() > 1 && chain.iter().any(|sha| !decided.contains(*sha)))
        .map(|(path, _)| *path)
        .collect();
    blame_paths.extend(renamed.iter().filter(|path| alive.contains(*path)));
    let blame_paths: Vec<&String> = blame_paths.into_iter().collect();
    let results: Vec<Result<BTreeMap<String, u64>, String>> = blame_paths
        .par_iter()
        .map(|path| blame_path(repo, cut, path, &in_range))
        .collect();
    let mut blamed_lines: BTreeMap<String, u64> = BTreeMap::new();
    for result in results {
        for (sha, lines) in result? {
            *blamed_lines.entry(sha).or_default() += lines;
        }
    }
    Ok(Ownership {
        last_toucher_paths,
        blamed_lines,
        blamed_paths: blame_paths.len() as u64,
    })
}

fn blame_path(
    repo: &Path,
    cut: &str,
    path: &str,
    in_range: &BTreeSet<&str>,
) -> Result<BTreeMap<String, u64>, String> {
    let output = git_bytes(
        repo,
        &["blame", "--line-porcelain", cut, "--", path],
        BLAME_TIMEOUT,
    )
    .map_err(|error| format!("git blame failed for {path}: {error}"))?;
    let text = String::from_utf8_lossy(&output);
    let mut lines: BTreeMap<String, u64> = BTreeMap::new();
    let mut expect_header = true;
    for line in text.split('\n') {
        if expect_header {
            let sha = line.split(' ').next().unwrap_or("");
            if is_sha(sha) {
                if in_range.contains(sha) {
                    *lines.entry(sha.to_string()).or_default() += 1;
                }
                expect_header = false;
            }
        } else if line.starts_with('\t') {
            expect_header = true;
        }
    }
    Ok(lines)
}

fn read_commits(repo: &Path, span: &str) -> Result<BTreeMap<String, Commit>, String> {
    let text = git_text(
        repo,
        &[
            "log",
            "--format=%H%x1f%P%x1f%T%x1f%s%x1e",
            "--no-show-signature",
            span,
        ],
    )?;
    let mut commits = BTreeMap::new();
    for entry in text.split('\x1e') {
        let entry = entry.trim_start_matches('\n');
        if entry.is_empty() {
            continue;
        }
        let fields: Vec<&str> = entry.split('\x1f').collect();
        let [sha, parents, tree, subject] = fields.as_slice() else {
            return Err(format!("unexpected git log record: {entry:?}"));
        };
        commits.insert(
            (*sha).to_string(),
            Commit {
                parents: shas(parents),
                tree: (*tree).to_string(),
                subject: (*subject).to_string(),
            },
        );
    }
    Ok(commits)
}

/// Changed paths against each commit's first parent. Renames are not detected,
/// so a rename is a delete plus an add, which keeps path identity exact.
fn read_touched_paths(
    repo: &Path,
    reachable: &[String],
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut touched: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for chunk in reachable.chunks(SHA_CHUNK) {
        let mut args = vec![
            "log",
            "--no-walk=unsorted",
            "-m",
            "--first-parent",
            "--no-renames",
            "--name-status",
            "-z",
            "--format=%x1e%H",
        ];
        args.extend(chunk.iter().map(String::as_str));
        let bytes = git_bytes(repo, &args, GIT_TIMEOUT)?;
        let text = String::from_utf8(bytes)
            .map_err(|_error| "git log produced a non-UTF-8 path".to_string())?;
        for entry in text.split('\x1e').filter(|entry| !entry.is_empty()) {
            let mut fields = entry.split('\0');
            let sha = fields.next().unwrap_or("").to_string();
            let rest: Vec<&str> = fields.collect();
            let paths = touched.entry(sha).or_default();
            // Git separates records with a newline that lands on the next status
            // token; path tokens are kept byte for byte.
            let mut tokens = rest.iter();
            while let (Some(status), Some(path)) = (tokens.next(), tokens.next()) {
                if status.trim_start_matches('\n').is_empty() {
                    break;
                }
                paths.insert((*path).to_string());
            }
        }
    }
    Ok(touched)
}

/// Destinations of renames git detects in each commit; their history lives
/// under another name, so only blame can attribute their lines.
fn read_rename_destinations(repo: &Path, reachable: &[String]) -> Result<BTreeSet<String>, String> {
    let mut destinations = BTreeSet::new();
    for chunk in reachable.chunks(SHA_CHUNK) {
        let mut args = vec![
            "log",
            "--no-walk=unsorted",
            "-m",
            "--first-parent",
            "-M",
            "--diff-filter=R",
            "--name-status",
            "-z",
            "--format=%x1e%H",
        ];
        args.extend(chunk.iter().map(String::as_str));
        let bytes = git_bytes(repo, &args, GIT_TIMEOUT)?;
        let text = String::from_utf8(bytes)
            .map_err(|_error| "git log produced a non-UTF-8 path".to_string())?;
        for entry in text.split('\x1e').filter(|entry| !entry.is_empty()) {
            let mut tokens = entry.split('\0').skip(1);
            while let (Some(status), Some(_old), Some(new)) =
                (tokens.next(), tokens.next(), tokens.next())
            {
                if status.trim_start_matches('\n').starts_with('R') {
                    destinations.insert(new.to_string());
                }
            }
        }
    }
    Ok(destinations)
}

fn alive_paths(repo: &Path, cut: &str) -> Result<BTreeSet<String>, String> {
    let bytes = git_bytes(
        repo,
        &["ls-tree", "-r", "--name-only", "-z", cut],
        GIT_TIMEOUT,
    )?;
    let text = String::from_utf8(bytes)
        .map_err(|_error| "git ls-tree produced a non-UTF-8 path".to_string())?;
    Ok(text
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

fn rev_parse(repo: &Path, rev: &str) -> Result<String, String> {
    let output = git_text(repo, &["rev-parse", "--verify", rev])?;
    let sha = output.trim().to_string();
    if is_sha(&sha) {
        Ok(sha)
    } else {
        Err(format!("`{rev}` did not resolve to a 40-hex SHA"))
    }
}

fn shas(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

fn git_text(repo: &Path, args: &[&str]) -> Result<String, String> {
    let bytes = git_bytes(repo, args, GIT_TIMEOUT)?;
    String::from_utf8(bytes).map_err(|_error| format!("git {} produced non-UTF-8 output", args[0]))
}

fn git_bytes(repo: &Path, args: &[&str], timeout: Duration) -> Result<Vec<u8>, String> {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        Path::new("git"),
        &owned,
        repo,
        &[],
        &[],
        timeout,
        &format!("git {}", args.first().copied().unwrap_or("")),
    )?;
    if output.timed_out {
        return Err(format!(
            "git {} timed out",
            args.first().copied().unwrap_or("")
        ));
    }
    match output.status {
        Some(status) if status.success() => Ok(output.stdout),
        _ => Err(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}
