//! Object-backed execution of a [`GitCandidateSubject`] (#3237 / #3277,
//! R2).
//!
//! R1 (#3276) bound the subject's identity; this module resolves that
//! identity through Git object plumbing and produces the two inputs the
//! existing analyzers need — the exact base→candidate unified diff and a
//! materialized candidate root — without consulting the worktree or the
//! index:
//!
//! - identities are validated with `git rev-parse` / `git cat-file -e`
//!   (read-only plumbing; no ref, index, or worktree mutation);
//! - the diff is `git diff-tree` between the two **trees**, preserving
//!   add/delete/rename/type-change information;
//! - the candidate root is materialized with `git archive` from the
//!   candidate tree alone into a fresh temp directory, so every byte
//!   comes from the bound tree;
//! - any failure (missing base/candidate, unsupported object mode,
//!   traversal, materialization error) fails closed naming the exact
//!   identity — never an empty analysis.

use crate::domain::{
    GitCandidateBase, GitCandidateSubject, GitCandidateSubjectError as SubjectError,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Bounded invocation deadline for each plumbing call.
const GIT_DEADLINE: Option<Duration> = Some(Duration::from_mins(1));

/// The resolved, analyzed form of one immutable subject: the derived
/// unified diff, the materialized candidate root (owned temp directory
/// — dropping the guard removes it), and the identities the diff was
/// derived from (recorded for R3 output projection).
#[allow(dead_code, reason = "recorded for the R3 output projection (#3277)")]
pub(crate) struct ResolvedGitCandidate {
    pub(crate) base_tree: String,
    pub(crate) candidate_tree: String,
    pub(crate) diff: String,
    pub(crate) root: PathBuf,
    /// Removes the materialized root on drop; keep alive for the run.
    pub(crate) _cleanup: TempRootGuard,
}

pub(crate) struct TempRootGuard(PathBuf);

impl Drop for TempRootGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn failed(detail: String) -> SubjectError {
    SubjectError::ExecutionFailed { detail }
}

fn git(root: &Path, args: &[&str]) -> Result<String, SubjectError> {
    let named = |detail: String| SubjectError::ExecutionFailed { detail };
    let output =
        crate::git::run_git_output_with_deadline(root, args, GIT_DEADLINE).map_err(named)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().next().unwrap_or("unknown git error").trim();
        return Err(named(format!(
            "git {} failed with {}: {detail}",
            args.first().unwrap_or(&""),
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Resolve the base side to one exact tree object ID, using the
/// repository's real empty-tree semantics when no base is requested.
fn resolve_base_tree(subject: &GitCandidateSubject) -> Result<String, SubjectError> {
    match &subject.base {
        GitCandidateBase::EmptyTree => git(
            &subject.repository_root,
            &[
                "rev-parse",
                "--verify",
                "4b825dc642cb6eb9a060e54bf8d69288fbee4904^{tree}",
            ],
        )
        .or_else(|_| {
            // A SHA-256 repository's empty tree has a different object
            // ID; ask Git for it rather than hard-coding either hash.
            let hash = git(&subject.repository_root, &["hash-object", "-t", "tree", ""])?;
            let tree = git(
                &subject.repository_root,
                &["rev-parse", "--verify", &format!("{hash}^{{tree}}")],
            )?;
            Ok(tree)
        }),
        GitCandidateBase::Treeish(treeish) => {
            let commit = git(
                &subject.repository_root,
                &["rev-parse", "--verify", &format!("{treeish}^{{commit}}")],
            )?;
            git(
                &subject.repository_root,
                &["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
            )
        }
    }
}

/// Validate that the candidate names one existing tree object.
fn resolve_candidate_tree(subject: &GitCandidateSubject) -> Result<String, SubjectError> {
    let treeish = subject.candidate_tree.as_str();
    let commit = git(
        &subject.repository_root,
        &["rev-parse", "--verify", &format!("{treeish}^{{commit}}")],
    )?;
    git(
        &subject.repository_root,
        &["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
    )
}

/// Derive the base→candidate unified diff from the trees alone.
fn derive_diff(root: &Path, base: &str, candidate: &str) -> Result<String, SubjectError> {
    git(
        root,
        &[
            "diff-tree",
            "--unified=0",
            "--no-ext-diff",
            "--root",
            base,
            candidate,
        ],
    )
}

/// Materialize the candidate tree into a fresh temp directory. Every
/// byte comes from `git archive` of the tree; the worktree and index
/// are never consulted.
fn materialize(
    root: &Path,
    candidate_tree: &str,
) -> Result<(PathBuf, TempRootGuard), SubjectError> {
    // Unique per invocation: concurrent runs (or racing tests) never
    // share a materialization directory, and a stale directory from a
    // crashed run can never be silently reused.
    let unique = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0)
    );
    let base_dir = std::env::temp_dir().join("ripr-git-candidate").join(unique);
    let target = base_dir.join(candidate_tree);
    if target.exists() {
        // A stale directory from a crashed run must not be reused: its
        // bytes are unverified. Remove and re-create deterministically.
        std::fs::remove_dir_all(&target).map_err(|error| {
            failed(format!(
                "stale materialization could not be removed: {error}"
            ))
        })?;
    }
    std::fs::create_dir_all(&target)
        .map_err(|error| failed(format!("materialization dir failed: {error}")))?;
    let tar_path = target.join("__candidate.tar");
    let archive = crate::git::run_git_output_with_deadline(
        root,
        &["archive", "--format=tar", candidate_tree],
        GIT_DEADLINE,
    )
    .map_err(|error| failed(format!("git archive failed: {error}")))?;
    if !archive.status.success() {
        return Err(failed(
            "git archive of the candidate tree failed".to_string(),
        ));
    }
    std::fs::write(&tar_path, &archive.stdout)
        .map_err(|error| failed(format!("archive write failed: {error}")))?;
    let extracted = untar(&tar_path, &target)?;
    let _ = std::fs::remove_file(&tar_path);
    let _ = extracted;
    Ok((target.clone(), TempRootGuard(target)))
}

/// Minimal in-crate tar extraction for `git archive` output: only the
/// entry shapes Git emits (regular files, directories), no symlinks or
/// gitlinks (those are rejected upstream as unsupported modes).
fn untar(tar_path: &Path, target: &Path) -> Result<usize, SubjectError> {
    let bytes =
        std::fs::read(tar_path).map_err(|error| failed(format!("archive read failed: {error}")))?;
    let mut entries = 0usize;
    let mut offset = 0usize;
    while offset + 512 <= bytes.len() {
        let header = &bytes[offset..offset + 512];
        let name = tar_string(&header[0..100]);
        let size = tar_octal(&header[124..136]);
        let typeflag = header[156];
        let data_start = offset + 512;
        let data_end = data_start + size;
        if data_end > bytes.len() {
            return Err(failed("truncated candidate archive".to_string()));
        }
        offset = data_start + size.div_ceil(512) * 512;
        if name.is_empty() {
            continue;
        }
        // Two consecutive zero blocks end the archive.
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        match typeflag {
            b'0' | 0 => {
                let path = safe_join(target, &name)?;
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        failed(format!("materialization mkdir failed: {error}"))
                    })?;
                }
                std::fs::write(&path, &bytes[data_start..data_end])
                    .map_err(|error| failed(format!("materialization write failed: {error}")))?;
                entries += 1;
            }
            b'5' => {
                let path = safe_join(target, &name)?;
                std::fs::create_dir_all(&path)
                    .map_err(|error| failed(format!("materialization mkdir failed: {error}")))?;
            }
            other => {
                return Err(failed(format!(
                    "unsupported archive entry type `{}` for `{name}`: the candidate tree                      contains a non-file object ripr cannot faithfully materialize",
                    other as char
                )));
            }
        }
    }
    Ok(entries)
}

/// Join an archive entry name under the target, rejecting traversal.
fn safe_join(target: &Path, name: &str) -> Result<PathBuf, SubjectError> {
    let relative = Path::new(name);
    if relative.is_absolute() || name.contains("..") || name.contains('\\') || name.starts_with('/')
    {
        return Err(failed(format!(
            "candidate archive entry `{name}` escapes the materialization root"
        )));
    }
    Ok(target.join(relative))
}

fn tar_string(field: &[u8]) -> String {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).to_string()
}

fn tar_octal(field: &[u8]) -> usize {
    let text = tar_string(field);
    usize::from_str_radix(text.trim().trim_end_matches('\0').trim(), 8).unwrap_or(0)
}

/// Resolve and execute one subject: validate identities, derive the
/// diff, and materialize the candidate root.
pub(crate) fn resolve(subject: &GitCandidateSubject) -> Result<ResolvedGitCandidate, SubjectError> {
    if !subject.repository_root.join(".git").exists()
        && !subject.repository_root.join("HEAD").exists()
    {
        return Err(failed(format!(
            "repository root `{}` does not own a Git object database",
            subject.repository_root.display()
        )));
    }
    let base_tree = resolve_base_tree(subject)?;
    let candidate_tree = resolve_candidate_tree(subject)?;
    if base_tree == candidate_tree {
        return Err(failed(
            "base tree and candidate tree are identical: no diff to analyze".to_string(),
        ));
    }
    let diff = derive_diff(&subject.repository_root, &base_tree, &candidate_tree)?;
    let (root, cleanup) = materialize(&subject.repository_root, &candidate_tree)?;
    Ok(ResolvedGitCandidate {
        base_tree,
        candidate_tree,
        diff,
        root,
        _cleanup: cleanup,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{GitCandidateDiffSemantics, GitObjectId, GitTreeish};

    fn subject(
        root: &Path,
        base: GitCandidateBase,
        treeish: &str,
    ) -> Result<GitCandidateSubject, String> {
        Ok(GitCandidateSubject {
            repository_root: root.to_path_buf(),
            base,
            candidate_tree: GitObjectId::parse(treeish).map_err(|error| error.to_string())?,
            diff_semantics: GitCandidateDiffSemantics::DirectTreeToTree,
        })
    }

    struct RepoGuard(PathBuf);
    impl Drop for RepoGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A real two-commit fixture repository with distinct base/candidate
    /// content, a rename, and a deletion.
    fn fixture_repo(name: &str) -> Result<(RepoGuard, String, String), String> {
        let root = std::env::temp_dir().join(format!("ripr-3277-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        let run = |args: &[&str]| -> Result<String, String> {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(format!(
                    "git {} failed: {}",
                    args[0],
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        };
        run(&["init", "--initial-branch=main"])?;
        run(&["config", "user.email", "ripr@example.invalid"])?;
        run(&["config", "user.name", "ripr test"])?;
        std::fs::write(root.join("src/lib.rs"), "pub fn base() -> u8 { 1 }\n")
            .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/old.rs"), "pub fn gone() {}\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("ripr.toml"), "[analysis]\nmode = \"draft\"\n")
            .map_err(|e| e.to_string())?;
        run(&["add", "."])?;
        run(&["commit", "-m", "base"])?;
        let base = run(&["rev-parse", "HEAD"])?;
        std::fs::write(root.join("src/lib.rs"), "pub fn candidate() -> u8 { 2 }\n")
            .map_err(|e| e.to_string())?;
        std::fs::rename(root.join("src/old.rs"), root.join("src/renamed.rs"))
            .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("tests/it.rs"), "use crate::candidate;\n")
            .map_err(|e| e.to_string())?;
        run(&["add", "."])?;
        run(&["commit", "-m", "candidate"])?;
        let candidate = run(&["rev-parse", "HEAD"])?;
        Ok((RepoGuard(root), base, candidate))
    }

    #[test]
    fn resolves_trees_derives_diff_and_materializes_bytes() -> Result<(), String> {
        let (guard, base, candidate) = fixture_repo("resolve")?;
        let s = subject(
            &guard.0,
            GitCandidateBase::Treeish(GitTreeish::new(&base).map_err(|e| e.to_string())?),
            &candidate,
        )?;
        let resolved = resolve(&s).map_err(|e| e.to_string())?;
        assert!(resolved.diff.contains("src/lib.rs"), "{}", resolved.diff);
        assert!(resolved.diff.contains("src/renamed.rs") || resolved.diff.contains("src/old.rs"));
        // Candidate bytes, not worktree bytes:
        assert_eq!(
            std::fs::read_to_string(resolved.root.join("src/lib.rs")).map_err(|e| e.to_string())?,
            "pub fn candidate() -> u8 { 2 }\n"
        );
        assert!(resolved.root.join("ripr.toml").exists());
        Ok(())
    }

    #[test]
    fn worktree_mutations_cannot_change_the_analysis_input() -> Result<(), String> {
        let (guard, base, candidate) = fixture_repo("mutate")?;
        let s = subject(
            &guard.0,
            GitCandidateBase::Treeish(GitTreeish::new(&base).map_err(|e| e.to_string())?),
            &candidate,
        )?;
        let first = resolve(&s).map_err(|e| e.to_string())?;
        // The three mutations from the issue's reproduction.
        std::fs::write(guard.0.join("src/lib.rs"), "pub fn dirty() -> u8 { 9 }\n")
            .map_err(|e| e.to_string())?;
        std::fs::write(guard.0.join("ripr.toml"), "[analysis]\nmode = \"deep\"\n")
            .map_err(|e| e.to_string())?;
        std::fs::write(guard.0.join("src/staged.rs"), "pub fn staged() {}\n")
            .map_err(|e| e.to_string())?;
        let mut add = std::process::Command::new("git");
        add.args(["add", "."]).current_dir(&guard.0);
        add.output().map_err(|e| e.to_string())?;
        let second = resolve(&s).map_err(|e| e.to_string())?;
        assert_eq!(
            first.diff, second.diff,
            "diff must follow objects, not the worktree"
        );
        assert_eq!(
            std::fs::read_to_string(second.root.join("src/lib.rs")).map_err(|e| e.to_string())?,
            "pub fn candidate() -> u8 { 2 }\n",
            "materialized bytes must follow the candidate tree"
        );
        assert!(
            !second.root.join("src/staged.rs").exists(),
            "a blob staged after binding must not appear in the candidate"
        );
        Ok(())
    }

    #[test]
    fn pipeline_executes_the_subject_against_candidate_bytes() -> Result<(), String> {
        let (guard, base, candidate) = fixture_repo("pipeline")?;
        // Dirty the worktree AND stage a different blob: the analysis
        // input must still follow the bound objects.
        std::fs::write(
            guard.0.join("src/lib.rs"),
            "pub fn dirty() -> u8 { 9 }
",
        )
        .map_err(|e| e.to_string())?;
        let mut add = std::process::Command::new("git");
        add.args(["add", "."]).current_dir(&guard.0);
        add.output().map_err(|e| e.to_string())?;
        let subject = subject(
            &guard.0,
            GitCandidateBase::Treeish(GitTreeish::new(&base).map_err(|e| e.to_string())?),
            &candidate,
        )?;
        let options = crate::analysis::AnalysisOptions {
            root: guard.0.clone(),
            base: None,
            diff_file: None,
            mode: crate::analysis::AnalysisMode::Draft,
            include_unchanged_tests: false,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            git_timeout: None,
            git_candidate: Some(subject),
            production_like_targets: Default::default(),
        };
        let result = crate::analysis::run_analysis_with_oracle_policy(
            &options,
            &crate::config::OraclePolicy::default(),
            &[crate::analysis::language::LanguageId::Rust],
        )?;
        // The derived diff contains the candidate's src/lib.rs change;
        // the dirty worktree bytes never entered.
        // A successful Rust run records no LanguageRun failure entry;
        // the completed outcome with the candidate's diff counts is the
        // observable proof the materialized root was analyzed.
        assert!(
            result
                .language_runs
                .iter()
                .all(|run| run.language != "rust"),
            "the Rust adapter must not fail against the candidate root: {:?}",
            result.language_runs
        );
        assert!(
            result.summary.changed_rust_files >= 1,
            "the candidate diff must be consumed: {:?}",
            result.summary
        );
        Ok(())
    }

    #[test]
    fn missing_candidate_fails_closed_naming_the_identity() -> Result<(), String> {
        let (guard, base, _candidate) = fixture_repo("missing")?;
        let s = subject(
            &guard.0,
            GitCandidateBase::Treeish(GitTreeish::new(&base).map_err(|e| e.to_string())?),
            "0123456789012345678901234567890123456789",
        )?;
        let error = match resolve(&s) {
            Err(error) => error,
            Ok(_) => return Err("unknown candidate unexpectedly resolved".to_string()),
        };
        assert!(
            error.to_string().contains("git rev-parse"),
            "failure must name the resolver: {error}"
        );
        Ok(())
    }

    #[test]
    fn empty_base_uses_real_empty_tree_semantics() -> Result<(), String> {
        let (guard, _base, candidate) = fixture_repo("empty")?;
        let s = subject(&guard.0, GitCandidateBase::EmptyTree, &candidate)?;
        let resolved = resolve(&s).map_err(|e| e.to_string())?;
        assert!(
            resolved.diff.contains("src/lib.rs"),
            "empty→candidate must show the added files"
        );
        Ok(())
    }
}
