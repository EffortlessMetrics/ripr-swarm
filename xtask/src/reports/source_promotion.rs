//! Exact-input, read-only source/swarm promotion preflight.
//!
//! This command is deliberately a preflight only. It validates two named
//! repository identities and exact commit inputs, then performs `git
//! merge-tree` in a disposable repository containing fetched objects. It
//! never changes either caller checkout and never creates the source join.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: &str = "ripr.source_promotion_preflight.v1";
const REPORT_JSON: &str = "source-promotion-preflight.json";
const REPORT_MD: &str = "source-promotion-preflight.md";
const DEFAULT_OUT: &str = "target/ripr/source-promotion";

#[derive(Clone, Debug)]
struct Options {
    source_parent: String,
    swarm_parent: String,
    swarm_ref: String,
    source_repo: PathBuf,
    swarm_repo: PathBuf,
    source_main: String,
    swarm_main: String,
    version: String,
    resolved_tree: Option<String>,
    out: PathBuf,
    source_remote: String,
    swarm_remote: String,
}

#[derive(Clone, Debug, Serialize)]
struct RepositoryIdentity {
    role: String,
    remote: String,
    expected_remote: String,
    git_common_dir: String,
    immutable_ref: Option<String>,
    immutable_ref_sha: Option<String>,
    identity: String,
    root_verified: bool,
    remote_verified: bool,
}

#[derive(Clone, Debug, Serialize)]
struct CommitRange {
    all_reachable_count: usize,
    first_parent_count: usize,
    all_reachable_sha256: String,
    first_parent_ordered_sha256: String,
    all_reachable_ordered_recipe: String,
    first_parent_ordered_recipe: String,
}

#[derive(Clone, Debug, Serialize)]
struct VersionSide {
    workspace_version: Option<String>,
    crate_version: Option<String>,
    extension_version: Option<String>,
    cargo_lock_package_version: Option<String>,
    npm_lock_root_version: Option<String>,
    changelog_mentions_version: bool,
}

#[derive(Clone, Debug, Serialize)]
struct VersionState {
    requested_version: String,
    source: VersionSide,
    swarm: VersionSide,
}

#[derive(Clone, Debug, Serialize)]
struct DryMerge {
    status: String,
    preview_tree: Option<String>,
    reviewed_resolved_tree: Option<String>,
    reviewed_resolved_tree_verified: bool,
    conflicts: Vec<String>,
    diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct Receipt {
    schema: String,
    mode: String,
    source_parent: String,
    swarm_parent: String,
    swarm_ref: String,
    swarm_ref_sha: String,
    source_main: String,
    swarm_main: String,
    merge_base: String,
    source_repository: RepositoryIdentity,
    swarm_repository: RepositoryIdentity,
    source_range: CommitRange,
    swarm_range: CommitRange,
    source_survivors: Vec<String>,
    swarm_only_paths: Vec<String>,
    swarm_authority_resolution_candidates: Vec<String>,
    dry_merge: DryMerge,
    version_state: VersionState,
    invalidation_rules: Vec<String>,
    next_commands: Vec<String>,
}

pub(crate) fn source_promotion(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let receipt = build_receipt(&options)?;
    let json = serde_json::to_string_pretty(&receipt)
        .map_err(|error| format!("failed to serialize source-promotion receipt: {error}"))?;
    let markdown = render_markdown(&receipt);
    fs::create_dir_all(&options.out)
        .map_err(|error| format!("failed to create {}: {error}", options.out.display()))?;
    fs::write(options.out.join(REPORT_JSON), format!("{json}\n"))
        .map_err(|error| format!("failed to write source-promotion JSON: {error}"))?;
    fs::write(options.out.join(REPORT_MD), markdown)
        .map_err(|error| format!("failed to write source-promotion Markdown: {error}"))?;
    println!("Wrote {}", options.out.join(REPORT_JSON).display());
    println!("Wrote {}", options.out.join(REPORT_MD).display());
    Ok(())
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    if args.first().map(String::as_str) != Some("preflight") {
        return Err(usage());
    }
    let mut values = BTreeMap::new();
    let mut index = 1;
    while index < args.len() {
        let key = args[index].as_str();
        if !key.starts_with("--") || index + 1 >= args.len() {
            return Err(usage());
        }
        if !matches!(
            key,
            "--source-parent"
                | "--swarm-parent"
                | "--swarm-ref"
                | "--source-repo"
                | "--swarm-repo"
                | "--source-main"
                | "--swarm-main"
                | "--version"
                | "--resolved-tree"
                | "--source-remote"
                | "--swarm-remote"
                | "--out"
        ) {
            return Err(format!("unknown option {key}\n{}", usage()));
        }
        if values.contains_key(key) {
            return Err(format!("duplicate option {key}\n{}", usage()));
        }
        values.insert(key, args[index + 1].clone());
        index += 2;
    }
    let required = |key: &str| -> Result<String, String> {
        values
            .get(key)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .ok_or_else(|| format!("missing {key}\n{}", usage()))
    };
    let source_parent = required("--source-parent")?;
    let swarm_parent = required("--swarm-parent")?;
    let swarm_ref = required("--swarm-ref")?;
    validate_sha("--source-parent", &source_parent)?;
    validate_sha("--swarm-parent", &swarm_parent)?;
    let version = required("--version")?;
    validate_swarm_ref(&swarm_ref, &version, &swarm_parent)?;
    let source_repo = PathBuf::from(required("--source-repo")?);
    let swarm_repo = PathBuf::from(required("--swarm-repo")?);
    let resolved_tree = values.get("--resolved-tree").cloned();
    if let Some(tree) = &resolved_tree {
        validate_sha("--resolved-tree", tree)?;
    }
    Ok(Options {
        source_parent,
        swarm_parent,
        swarm_ref,
        source_repo,
        swarm_repo,
        source_main: values
            .get("--source-main")
            .cloned()
            .unwrap_or_else(|| "origin/main".to_string()),
        swarm_main: values
            .get("--swarm-main")
            .cloned()
            .unwrap_or_else(|| "origin/main".to_string()),
        version,
        resolved_tree,
        out: values
            .get("--out")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_OUT)),
        source_remote: values
            .get("--source-remote")
            .cloned()
            .unwrap_or_else(|| "EffortlessMetrics/ripr".to_string()),
        swarm_remote: values
            .get("--swarm-remote")
            .cloned()
            .unwrap_or_else(|| "EffortlessMetrics/ripr-swarm".to_string()),
    })
}

fn usage() -> String {
    "usage: cargo xtask source-promotion preflight --source-parent <full-sha> --swarm-parent <full-sha> --swarm-ref <immutable-ref> --source-repo <path> --swarm-repo <path> --version <version> [--resolved-tree <full-tree-sha>] [--source-main <rev>] [--swarm-main <rev>] [--source-remote <owner/repo>] [--swarm-remote <owner/repo>] [--out <dir>]".to_string()
}

fn validate_sha(name: &str, value: &str) -> Result<(), String> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} must be a complete 40-character hexadecimal commit SHA"
        ));
    }
    Ok(())
}

fn validate_swarm_ref(reference: &str, version: &str, parent: &str) -> Result<(), String> {
    let expected = format!("refs/ripr/release-{version}-{parent}");
    if reference != expected {
        return Err(format!(
            "--swarm-ref must use the governed immutable format {expected}"
        ));
    }
    Ok(())
}

fn build_receipt(options: &Options) -> Result<Receipt, String> {
    let source = inspect_repository(
        "source",
        &options.source_repo,
        &options.source_remote,
        &options.source_parent,
        &options.source_main,
        None,
    )?;
    let swarm = inspect_repository(
        "swarm",
        &options.swarm_repo,
        &options.swarm_remote,
        &options.swarm_parent,
        &options.swarm_main,
        Some(&options.swarm_ref),
    )?;
    let source_root = repository_root(&options.source_repo)?;
    let swarm_root = repository_root(&options.swarm_repo)?;
    if source_root == swarm_root || source.common_dir == swarm.common_dir {
        return Err(
            "source and swarm must be distinct repositories; refusing an ambiguous identity"
                .to_string(),
        );
    }
    let reviewed_resolved_tree_verified = options
        .resolved_tree
        .as_deref()
        .map(|tree| verify_tree_in_repositories(&source_root, &swarm_root, tree))
        .transpose()?
        .is_some();
    let (merge_base, source_range, swarm_range, source_paths, swarm_paths, dry_merge) =
        with_disposable_repo(
            &source_root,
            &swarm_root,
            &options.source_parent,
            &options.swarm_parent,
            |repo| {
                let merge_base = git(
                    repo,
                    &["merge-base", &options.source_parent, &options.swarm_parent],
                )?
                .trim()
                .to_string();
                let source_range = commit_range(repo, &merge_base, &options.source_parent)?;
                let swarm_range = commit_range(repo, &merge_base, &options.swarm_parent)?;
                let source_paths = changed_paths(repo, &merge_base, &options.source_parent)?;
                let swarm_paths = changed_paths(repo, &merge_base, &options.swarm_parent)?;
                let dry_merge = dry_merge_from_repo(
                    repo,
                    &options.source_parent,
                    &options.swarm_parent,
                    options.resolved_tree.as_deref(),
                    reviewed_resolved_tree_verified,
                )?;
                Ok((
                    merge_base,
                    source_range,
                    swarm_range,
                    source_paths,
                    swarm_paths,
                    dry_merge,
                ))
            },
        )?;
    let mut swarm_authority_resolution_candidates = swarm_paths
        .iter()
        .filter(|path| is_swarm_authority_path(path))
        .cloned()
        .collect::<Vec<_>>();
    swarm_authority_resolution_candidates.sort();
    Ok(Receipt {
        schema: SCHEMA.to_string(),
        mode: if merge_base == options.source_parent {
            "fast_forward".to_string()
        } else {
            "two_parent_join".to_string()
        },
        source_parent: options.source_parent.clone(),
        swarm_parent: options.swarm_parent.clone(),
        swarm_ref: options.swarm_ref.clone(),
        swarm_ref_sha: swarm
            .identity
            .immutable_ref_sha
            .clone()
            .ok_or_else(|| "swarm immutable ref was not resolved".to_string())?,
        source_main: source.resolved_main,
        swarm_main: swarm.resolved_main,
        merge_base,
        source_repository: source.identity,
        swarm_repository: swarm.identity,
        source_range,
        swarm_range,
        source_survivors: source_paths,
        swarm_only_paths: swarm_paths,
        swarm_authority_resolution_candidates,
        dry_merge,
        version_state: VersionState {
            requested_version: options.version.clone(),
            source: version_side(&source_root, &options.source_parent, &options.version),
            swarm: version_side(&swarm_root, &options.swarm_parent, &options.version),
        },
        invalidation_rules: vec![
            "Any change to SOURCE_PARENT, SWARM_PARENT, SWARM_REF resolution, source main, or swarm main invalidates this receipt; regenerate rather than editing it.".to_string(),
            "A changed repository remote, merge base, ancestry count, ordered SHA digest, conflict list, or resolved tree requires a fresh receipt.".to_string(),
            "This receipt does not construct or prove the source join, version metadata, qualification, publication, or back-sync.".to_string(),
        ],
        next_commands: vec![
            "Review every dry-merge conflict and semantic overlap before constructing a join.".to_string(),
            "Pass this exact parent pair, validated SWARM_REF, and receipt to source preflight; do not substitute a branch name or later main.".to_string(),
        ],
    })
}

struct InspectedRepository {
    identity: RepositoryIdentity,
    common_dir: PathBuf,
    immutable_ref_sha: Option<String>,
    resolved_main: String,
}

fn inspect_repository(
    role: &str,
    repo: &Path,
    expected_remote: &str,
    parent: &str,
    main: &str,
    immutable_ref: Option<&str>,
) -> Result<InspectedRepository, String> {
    let root = repository_root(repo)?;
    let common_dir = git_common_dir(&root)?;
    let remote = git(&root, &["remote", "get-url", "origin"])?;
    let remote = remote.trim().to_string();
    if !remote_matches(&remote, expected_remote) {
        return Err(format!(
            "{role} repository origin `{remote}` does not match expected `{expected_remote}`"
        ));
    }
    let resolved_parent = exact_commit(&root, parent, role)?;
    let resolved_main = git(
        &root,
        &["rev-parse", "--verify", &format!("{main}^{{commit}}")],
    )?
    .trim()
    .to_string();
    if role == "source" && resolved_main != resolved_parent {
        return Err(format!(
            "source parent {parent} is not the declared current source main {resolved_main}; hold source main or repin"
        ));
    }
    if role == "swarm" && !git_status_ok(&root, &["merge-base", "--is-ancestor", parent, main]) {
        return Err(format!(
            "swarm parent {parent} is not reachable from declared swarm main {main}"
        ));
    }
    let immutable_ref_sha = immutable_ref
        .map(|reference| exact_ref_commit(&root, reference, parent, role))
        .transpose()?;
    Ok(InspectedRepository {
        identity: RepositoryIdentity {
            role: role.to_string(),
            remote,
            expected_remote: expected_remote.to_string(),
            git_common_dir: common_dir.display().to_string(),
            immutable_ref: immutable_ref.map(str::to_string),
            immutable_ref_sha: immutable_ref_sha.clone(),
            identity: "origin-remote-and-git-root-verified".to_string(),
            root_verified: true,
            remote_verified: true,
        },
        common_dir,
        immutable_ref_sha,
        resolved_main,
    })
}

fn repository_root(repo: &Path) -> Result<PathBuf, String> {
    let root = git(repo, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(root.trim());
    if !root.is_dir() {
        return Err(format!(
            "git repository root does not exist: {}",
            root.display()
        ));
    }
    Ok(root)
}

fn git_common_dir(root: &Path) -> Result<PathBuf, String> {
    let raw = git(root, &["rev-parse", "--git-common-dir"])?;
    let path = PathBuf::from(raw.trim());
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve git common directory {}: {error}",
            path.display()
        )
    })
}

fn exact_commit(repo: &Path, sha: &str, role: &str) -> Result<String, String> {
    let resolved = git(
        repo,
        &["rev-parse", "--verify", &format!("{sha}^{{commit}}")],
    )?
    .trim()
    .to_string();
    if resolved != sha {
        return Err(format!(
            "{role} parent must resolve to the exact supplied SHA {sha}"
        ));
    }
    Ok(resolved)
}

fn exact_ref_commit(
    repo: &Path,
    reference: &str,
    expected: &str,
    role: &str,
) -> Result<String, String> {
    let ref_spec = format!("{reference}^{{commit}}");
    let resolved = git(repo, &["rev-parse", "--verify", &ref_spec])?
        .trim()
        .to_string();
    if resolved != expected {
        return Err(format!(
            "{role} immutable ref {reference:?} resolves to {resolved}, not exact parent {expected}"
        ));
    }
    Ok(resolved)
}

fn remote_matches(actual: &str, expected: &str) -> bool {
    canonical_remote(actual).is_some() && canonical_remote(actual) == canonical_remote(expected)
}

fn canonical_remote(value: &str) -> Option<String> {
    let mut value = value.trim().to_ascii_lowercase();
    let prefix = [
        "https://github.com/",
        "ssh://git@github.com/",
        "git@github.com:",
    ]
    .into_iter()
    .find(|prefix| value.starts_with(prefix));
    if let Some(prefix) = prefix {
        value = value[prefix.len()..].to_string();
    } else if value.contains("://") || value.contains('@') {
        return None;
    }
    value = value.trim_end_matches('/').to_string();
    if value.ends_with(".git") {
        value.truncate(value.len() - ".git".len());
    }
    let mut segments = value.split('/');
    let owner = segments.next()?.trim();
    let repository = segments.next()?.trim();
    if segments.next().is_some()
        || owner.is_empty()
        || repository.is_empty()
        || owner == "."
        || owner == ".."
        || repository == "."
        || repository == ".."
        || owner
            .chars()
            .any(|character| matches!(character, '?' | '#'))
        || repository
            .chars()
            .any(|character| matches!(character, '?' | '#'))
    {
        return None;
    }
    Some(format!("{owner}/{repository}"))
}

fn commit_range(repo: &Path, base: &str, head: &str) -> Result<CommitRange, String> {
    let all = lines(git(
        repo,
        &[
            "rev-list",
            "--topo-order",
            "--reverse",
            &format!("{base}..{head}"),
        ],
    )?);
    let first = lines(git(
        repo,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            &format!("{base}..{head}"),
        ],
    )?);
    let first_forward = lines(git(
        repo,
        &["rev-list", "--first-parent", &format!("{base}..{head}")],
    )?);
    Ok(CommitRange {
        all_reachable_count: all.len(),
        first_parent_count: first.len(),
        all_reachable_sha256: digest_lines(&all),
        first_parent_ordered_sha256: digest_lines(&first),
        all_reachable_ordered_recipe:
            "git rev-list --topo-order --reverse MERGE_BASE..PARENT; UTF-8 SHA lines joined with LF; SHA-256".to_string(),
        first_parent_ordered_recipe: "git rev-list --first-parent --reverse MERGE_BASE..PARENT; UTF-8 SHA lines joined with LF; SHA-256".to_string(),
    })
    .and_then(|range| {
        if first_forward.len() != first.len() {
            Err("git produced inconsistent first-parent range output".to_string())
        } else {
            Ok(range)
        }
    })
}

fn digest_lines(lines: &[String]) -> String {
    let mut hasher = Sha256::new();
    for line in lines {
        hasher.update(line.as_bytes());
        hasher.update([b'\n']);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn changed_paths(repo: &Path, base: &str, head: &str) -> Result<Vec<String>, String> {
    let mut paths = lines(git(
        repo,
        &["diff", "--name-only", &format!("{base}..{head}")],
    )?);
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn is_swarm_authority_path(path: &str) -> bool {
    path.starts_with(".github/")
        || path == ".github/settings.yml"
        || path.starts_with("policy/")
        || path.starts_with(".ripr/")
}

fn with_disposable_repo<T>(
    source: &Path,
    swarm: &Path,
    source_parent: &str,
    swarm_parent: &str,
    action: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock before epoch: {error}"))?
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "ripr-source-promotion-{nonce}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create disposable merge repo: {error}"))?;
    let result = (|| {
        git(&path, &["init", "--quiet"])?;
        git(
            &path,
            &[
                "fetch",
                "--no-tags",
                source.to_string_lossy().as_ref(),
                source_parent,
            ],
        )?;
        git(
            &path,
            &[
                "fetch",
                "--no-tags",
                swarm.to_string_lossy().as_ref(),
                swarm_parent,
            ],
        )?;
        action(&path)
    })();
    let cleanup = fs::remove_dir_all(&path).map_err(|error| {
        format!(
            "failed to remove disposable merge repository {}: {error}",
            path.display()
        )
    });
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(action_error), Err(cleanup_error)) => {
            Err(format!("{action_error}; additionally, {cleanup_error}"))
        }
    }
}

fn dry_merge_from_repo(
    repo: &Path,
    source_parent: &str,
    swarm_parent: &str,
    reviewed_resolved_tree: Option<&str>,
    reviewed_resolved_tree_verified: bool,
) -> Result<DryMerge, String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args([
            "merge-tree",
            "--write-tree",
            "--messages",
            source_parent,
            swarm_parent,
        ])
        .output()
        .map_err(|error| format!("failed to execute disposable git merge-tree: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() && stdout.trim().is_empty() && stderr.trim().is_empty() {
        return Err(format!(
            "disposable git merge-tree failed ({})",
            output.status
        ));
    }
    let diagnostics = lines(format!("{stdout}{stderr}"));
    let conflicts = diagnostics
        .iter()
        .filter_map(|line| line.strip_prefix("CONFLICT "))
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    if !output.status.success() && conflicts.is_empty() {
        return Err(format!(
            "disposable git merge-tree failed ({}): {}",
            output.status,
            diagnostics.join(" | ")
        ));
    }
    let preview_tree = stdout
        .lines()
        .next()
        .filter(|line| line.len() == 40)
        .map(str::to_string);
    if output.status.success() && preview_tree.is_none() {
        return Err(format!(
            "disposable git merge-tree returned success without a merged tree: {}",
            diagnostics.join(" | ")
        ));
    }
    Ok(DryMerge {
        status: if conflicts.is_empty() {
            "clean"
        } else {
            "conflicts"
        }
        .to_string(),
        preview_tree,
        reviewed_resolved_tree: reviewed_resolved_tree.map(str::to_string),
        reviewed_resolved_tree_verified,
        conflicts,
        diagnostics,
    })
}

fn verify_tree_object(repo: &Path, tree: &str) -> Result<bool, String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["cat-file", "-t", tree])
        .output()
        .map_err(|error| format!("failed to inspect resolved tree {tree}: {error}"))?;
    if !output.status.success() {
        return Ok(false);
    }
    let object_type = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if object_type != "tree" {
        return Ok(false);
    }
    Ok(true)
}

fn verify_tree_in_repositories(source: &Path, swarm: &Path, tree: &str) -> Result<(), String> {
    if verify_tree_object(source, tree)? || verify_tree_object(swarm, tree)? {
        Ok(())
    } else {
        Err(format!(
            "resolved tree {tree} is not a tree object in either supplied repository object store"
        ))
    }
}

struct GitOutput {
    stdout: String,
}

fn run_git(repo: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|error| format!("failed to execute git {}: {error}", args.join(" ")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!(
            "git {} failed ({}): {}",
            args.join(" "),
            output.status,
            stderr.trim()
        ));
    }
    Ok(GitOutput { stdout })
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    Ok(run_git(repo, args)?.stdout)
}

fn git_status_ok(repo: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn lines(text: String) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn version_side(root: &Path, parent: &str, version: &str) -> VersionSide {
    let read = |relative: &str| git_show(root, parent, relative);
    let cargo = read("Cargo.toml");
    let crate_manifest = read("crates/ripr/Cargo.toml");
    let package = read("editors/vscode/package.json");
    let cargo_lock = read("Cargo.lock");
    let npm_lock = read("editors/vscode/package-lock.json");
    let changelog = read("CHANGELOG.md").unwrap_or_default();
    let parse_version = |text: Option<String>| {
        text.and_then(|body| {
            body.lines()
                .find_map(|line| line.trim().strip_prefix("version = \"")?.split('"').next())
                .map(str::to_string)
        })
    };
    let extension_version = package.and_then(|body| {
        body.lines().find_map(|line| {
            let line = line.trim();
            line.strip_prefix("\"version\": \"")?
                .split('"')
                .next()
                .map(str::to_string)
        })
    });
    let cargo_lock_package_version =
        cargo_lock.and_then(|body| lock_package_version(&body, "ripr"));
    let npm_lock_root_version = npm_lock.and_then(|body| {
        serde_json::from_str::<serde_json::Value>(&body)
            .ok()?
            .get("packages")?
            .get("")?
            .get("version")?
            .as_str()
            .map(str::to_string)
    });
    VersionSide {
        workspace_version: parse_version(cargo),
        crate_version: parse_version(crate_manifest),
        extension_version,
        cargo_lock_package_version,
        npm_lock_root_version,
        changelog_mentions_version: changelog.contains(&format!("[{version}]"))
            || changelog.contains(&format!("## {version}")),
    }
}

fn lock_package_version(text: &str, package_name: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            in_package = false;
            continue;
        }
        if trimmed == format!("name = \"{package_name}\"") {
            in_package = true;
            continue;
        }
        if in_package {
            if let Some(version) = trimmed
                .strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
            {
                return Some(version.to_string());
            }
            if trimmed.starts_with("[[") {
                in_package = false;
            }
        }
    }
    None
}

fn git_show(root: &Path, commit: &str, path: &str) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["show", &format!("{commit}:{path}")])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

fn render_markdown(receipt: &Receipt) -> String {
    let list = |items: &[String]| {
        if items.is_empty() {
            "- none\n".to_string()
        } else {
            items.iter().map(|item| format!("- `{item}`\n")).collect()
        }
    };
    format!(
        "# Source-promotion preflight\n\n- Schema: {}\n- Mode: {}\n- SOURCE_PARENT: {}\n- SWARM_PARENT: {}\n- SWARM_REF: {}\n- SWARM_REF_SHA: {}\n- MERGE_BASE: {}\n\n## Repository identity\n\n- source: {} (expected {}; git common dir {})\n- swarm: {} (expected {}; git common dir {})\n\n## Ancestry\n\n| range | all reachable | first parent | all digest | ordered first-parent digest |\n| --- | ---: | ---: | --- | --- |\n| source | {} | {} | {} | {} |\n| swarm | {} | {} | {} | {} |\n\nAll-reachable digest recipe: {}\nFirst-parent digest recipe: {}\n\n## Dry merge\n\nStatus: **{}**\nPreview tree (automatic, non-final): {:?}\nReviewed resolved tree: {:?}\nReviewed resolved tree verified in supplied repository object store: {}\n\nConflicts:\n{}\n\n## Source survivors\n\n{}\n## Swarm-only paths\n\n{}\n## Swarm authority-resolution candidates (non-dispositive)\n\n{}\n## Version state\n\n- requested: {}\n- source: workspace={:?}, crate={:?}, extension={:?}, cargo-lock-ripr={:?}, npm-lock-root={:?}, changelog-mentions={}\n- swarm: workspace={:?}, crate={:?}, extension={:?}, cargo-lock-ripr={:?}, npm-lock-root={:?}, changelog-mentions={}\n\n## Invalidation rules\n\n{}\n## Next commands\n\n{}",
        receipt.schema,
        receipt.mode,
        receipt.source_parent,
        receipt.swarm_parent,
        receipt.swarm_ref,
        receipt.swarm_ref_sha,
        receipt.merge_base,
        receipt.source_repository.remote,
        receipt.source_repository.expected_remote,
        receipt.source_repository.git_common_dir,
        receipt.swarm_repository.remote,
        receipt.swarm_repository.expected_remote,
        receipt.swarm_repository.git_common_dir,
        receipt.source_range.all_reachable_count,
        receipt.source_range.first_parent_count,
        receipt.source_range.all_reachable_sha256,
        receipt.source_range.first_parent_ordered_sha256,
        receipt.swarm_range.all_reachable_count,
        receipt.swarm_range.first_parent_count,
        receipt.swarm_range.all_reachable_sha256,
        receipt.swarm_range.first_parent_ordered_sha256,
        receipt.swarm_range.all_reachable_ordered_recipe,
        receipt.swarm_range.first_parent_ordered_recipe,
        receipt.dry_merge.status,
        receipt.dry_merge.preview_tree,
        receipt.dry_merge.reviewed_resolved_tree,
        receipt.dry_merge.reviewed_resolved_tree_verified,
        list(&receipt.dry_merge.conflicts),
        list(&receipt.source_survivors),
        list(&receipt.swarm_only_paths),
        list(&receipt.swarm_authority_resolution_candidates),
        receipt.version_state.requested_version,
        receipt.version_state.source.workspace_version,
        receipt.version_state.source.crate_version,
        receipt.version_state.source.extension_version,
        receipt.version_state.source.cargo_lock_package_version,
        receipt.version_state.source.npm_lock_root_version,
        receipt.version_state.source.changelog_mentions_version,
        receipt.version_state.swarm.workspace_version,
        receipt.version_state.swarm.crate_version,
        receipt.version_state.swarm.extension_version,
        receipt.version_state.swarm.cargo_lock_package_version,
        receipt.version_state.swarm.npm_lock_root_version,
        receipt.version_state.swarm.changelog_mentions_version,
        list(&receipt.invalidation_rules),
        list(&receipt.next_commands),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_abbreviated_parent() -> Result<(), String> {
        let result = parse_args(&[
            "preflight".to_string(),
            "--source-parent".to_string(),
            "deadbeef".to_string(),
        ]);
        if result.is_ok() {
            return Err("abbreviated SHA unexpectedly accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn requires_an_immutable_swarm_ref() -> Result<(), String> {
        let args = vec![
            "preflight".to_string(),
            "--source-parent".to_string(),
            "0000000000000000000000000000000000000000".to_string(),
            "--swarm-parent".to_string(),
            "1111111111111111111111111111111111111111".to_string(),
        ];
        let error = parse_args(&args)
            .err()
            .ok_or_else(|| "missing swarm ref was unexpectedly accepted".to_string())?;
        if !error.contains("--swarm-ref") {
            return Err(format!("missing-ref error omitted --swarm-ref: {error}"));
        }
        Ok(())
    }

    #[test]
    fn rejects_a_branch_or_wrongly_named_swarm_ref() -> Result<(), String> {
        let parent = "1111111111111111111111111111111111111111";
        for reference in ["main", "refs/heads/main", "refs/ripr/release-0.11.0-wrong"] {
            if validate_swarm_ref(reference, "0.11.0", parent).is_ok() {
                return Err(format!(
                    "non-governed immutable ref was accepted: {reference}"
                ));
            }
        }
        validate_swarm_ref(
            "refs/ripr/release-0.11.0-1111111111111111111111111111111111111111",
            "0.11.0",
            parent,
        )
    }

    #[test]
    fn digest_recipe_is_order_sensitive() -> Result<(), String> {
        let left = digest_lines(&["a".to_string(), "b".to_string()]);
        let right = digest_lines(&["b".to_string(), "a".to_string()]);
        if left == right {
            return Err("ordered digest ignored input order".to_string());
        }
        Ok(())
    }

    #[test]
    fn version_observation_reads_exact_parent_not_checkout_files() -> Result<(), String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock before epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-version-parent-{nonce}"));
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        test_git(&root, &["init", "--quiet"])?;
        test_git(&root, &["config", "user.email", "test@example.invalid"])?;
        test_git(&root, &["config", "user.name", "test"])?;
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.10.1\"\n",
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            root.join("Cargo.lock"),
            "[[package]]\nname = \"ripr\"\nversion = \"0.10.1\"\n",
        )
        .map_err(|error| error.to_string())?;
        fs::create_dir_all(root.join("editors/vscode")).map_err(|error| error.to_string())?;
        fs::write(
            root.join("editors/vscode/package.json"),
            "{\n  \"version\": \"0.10.1\"\n}\n",
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            root.join("editors/vscode/package-lock.json"),
            "{\n  \"packages\": {\"\": {\"version\": \"0.10.1\"}}\n}\n",
        )
        .map_err(|error| error.to_string())?;
        test_git(&root, &["add", "Cargo.toml"])?;
        test_git(&root, &["add", "Cargo.lock", "editors/vscode"])?;
        test_git(&root, &["commit", "--quiet", "-m", "version"])?;
        let parent = test_git_output(&root, &["rev-parse", "HEAD"])?;
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nversion = \"9.9.9\"\n",
        )
        .map_err(|error| error.to_string())?;
        let observed = version_side(&root, &parent, "0.11.0");
        if observed.workspace_version.as_deref() != Some("0.10.1") {
            return Err(format!(
                "version observation used checkout state: {observed:?}"
            ));
        }
        if observed.cargo_lock_package_version.as_deref() != Some("0.10.1")
            || observed.npm_lock_root_version.as_deref() != Some("0.10.1")
        {
            return Err(format!(
                "lockfile version observations were not read from parent: {observed:?}"
            ));
        }
        fs::remove_dir_all(&root).map_err(|error| format!("cleanup failed: {error}"))?;
        Ok(())
    }

    #[test]
    fn moved_or_missing_swarm_ref_is_rejected() -> Result<(), String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock before epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-swarm-ref-{nonce}"));
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        test_git(&root, &["init", "--quiet"])?;
        test_git(&root, &["config", "user.email", "test@example.invalid"])?;
        test_git(&root, &["config", "user.name", "test"])?;
        fs::write(root.join("file"), "one\n").map_err(|error| error.to_string())?;
        test_git(&root, &["add", "file"])?;
        test_git(&root, &["commit", "--quiet", "-m", "one"])?;
        let first = test_git_output(&root, &["rev-parse", "HEAD"])?;
        let reference = "refs/releases/test";
        test_git(&root, &["update-ref", reference, &first])?;
        fs::write(root.join("file"), "two\n").map_err(|error| error.to_string())?;
        test_git(&root, &["commit", "--quiet", "-am", "two"])?;
        let second = test_git_output(&root, &["rev-parse", "HEAD"])?;
        test_git(&root, &["update-ref", reference, &second])?;
        let moved = exact_ref_commit(&root, reference, &first, "swarm")
            .err()
            .ok_or_else(|| "moved immutable ref was unexpectedly accepted".to_string())?;
        if !moved.contains("not exact parent") {
            return Err(format!("unexpected moved-ref error: {moved}"));
        }
        if exact_ref_commit(&root, "refs/releases/missing", &first, "swarm").is_ok() {
            return Err("missing immutable ref was unexpectedly accepted".to_string());
        }
        fs::remove_dir_all(&root).map_err(|error| format!("cleanup failed: {error}"))?;
        Ok(())
    }

    #[test]
    fn remote_identity_normalizes_git_urls() -> Result<(), String> {
        if !remote_matches(
            "git@github.com:EffortlessMetrics/ripr-swarm.git",
            "EffortlessMetrics/ripr-swarm",
        ) {
            return Err("SSH remote was not normalized".to_string());
        }
        if remote_matches(
            "https://github.com/other/ripr.git",
            "EffortlessMetrics/ripr",
        ) {
            return Err("wrong remote was accepted".to_string());
        }
        for (actual, expected) in [
            (
                "https://github.com/evil/effortlessmetrics/ripr.git",
                "EffortlessMetrics/ripr",
            ),
            (
                "https://github.com/EffortlessMetrics/ripr-evil.git",
                "EffortlessMetrics/ripr",
            ),
            (
                "https://github.com/EffortlessMetrics/ripr?redirect=evil",
                "EffortlessMetrics/ripr",
            ),
        ] {
            if remote_matches(actual, expected) {
                return Err(format!("remote suffix trick was accepted: {actual}"));
            }
        }
        Ok(())
    }

    #[test]
    fn remote_identity_accepts_supported_windows_safe_forms() -> Result<(), String> {
        for actual in [
            "https://github.com/EffortlessMetrics/ripr-swarm.git",
            "ssh://git@github.com/EffortlessMetrics/ripr-swarm",
            "git@github.com:EffortlessMetrics/ripr-swarm.git",
        ] {
            if !remote_matches(actual, "EffortlessMetrics/ripr-swarm") {
                return Err(format!("supported remote form was rejected: {actual}"));
            }
        }
        Ok(())
    }

    #[test]
    fn authority_path_filter_is_narrow_and_explicit() -> Result<(), String> {
        if !is_swarm_authority_path(".github/settings.yml")
            || !is_swarm_authority_path("policy/process_allowlist.txt")
        {
            return Err("authority paths were not classified".to_string());
        }
        if is_swarm_authority_path("src/lib.rs") {
            return Err("product path was misclassified".to_string());
        }
        Ok(())
    }

    #[test]
    fn fixture_receipt_contract_is_discriminating() -> Result<(), String> {
        let text = fs::read_to_string("../fixtures/source_promotion/diverged-conflict.json")
            .map_err(|error| format!("failed to read source-promotion fixture: {error}"))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("failed to parse source-promotion fixture: {error}"))?;
        if value["expected"]["mode"] != "two_parent_join"
            || value["expected"]["dry_merge_status"] != "conflicts"
            || value["expected"]["source_first_parent_count"] != 1
            || value["expected"]["swarm_first_parent_count"] != 1
        {
            return Err(
                "fixture does not pin a divergent, conflicting two-parent case".to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn disposable_preflight_keeps_authoritative_checkouts_unchanged() -> Result<(), String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock before epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-preflight-fixture-{nonce}"));
        let source = root.join("source");
        let swarm = root.join("swarm");
        fs::create_dir_all(&source).map_err(|error| error.to_string())?;
        test_git(&source, &["init", "--quiet"])?;
        test_git(&source, &["config", "user.email", "test@example.invalid"])?;
        test_git(&source, &["config", "user.name", "test"])?;
        fs::write(source.join("shared.txt"), "base\n").map_err(|error| error.to_string())?;
        test_git(&source, &["add", "shared.txt"])?;
        test_git(&source, &["commit", "--quiet", "-m", "base"])?;
        let base = test_git_output(&source, &["rev-parse", "HEAD"])?;
        fs::write(source.join("shared.txt"), "source\n").map_err(|error| error.to_string())?;
        test_git(&source, &["commit", "--quiet", "-am", "source"])?;
        let source_parent = test_git_output(&source, &["rev-parse", "HEAD"])?;
        test_git(
            &source,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/EffortlessMetrics/ripr.git",
            ],
        )?;
        test_git(&source, &["clone", ".", swarm.to_string_lossy().as_ref()])?;
        test_git(&swarm, &["config", "user.email", "test@example.invalid"])?;
        test_git(&swarm, &["config", "user.name", "test"])?;
        test_git(&swarm, &["checkout", "--quiet", "-B", "main", &base])?;
        fs::write(swarm.join("shared.txt"), "swarm\n").map_err(|error| error.to_string())?;
        test_git(&swarm, &["commit", "--quiet", "-am", "swarm"])?;
        let swarm_parent = test_git_output(&swarm, &["rev-parse", "HEAD"])?;
        test_git(
            &swarm,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/EffortlessMetrics/ripr-swarm.git",
            ],
        )?;
        let source_head_before = test_git_output(&source, &["rev-parse", "HEAD"])?;
        let swarm_head_before = test_git_output(&swarm, &["rev-parse", "HEAD"])?;
        let swarm_ref = "refs/releases/test-swarm";
        test_git(&swarm, &["update-ref", swarm_ref, &swarm_parent])?;
        let receipt = build_receipt(&Options {
            source_parent,
            swarm_parent,
            swarm_ref: swarm_ref.to_string(),
            source_repo: source.clone(),
            swarm_repo: swarm.clone(),
            source_main: "HEAD".to_string(),
            swarm_main: "HEAD".to_string(),
            version: "0.11.0".to_string(),
            resolved_tree: None,
            out: root.join("out"),
            source_remote: "EffortlessMetrics/ripr".to_string(),
            swarm_remote: "EffortlessMetrics/ripr-swarm".to_string(),
        })?;
        if receipt.mode != "two_parent_join" || receipt.dry_merge.status != "conflicts" {
            return Err(
                "diverged fixture did not produce a conflicting two-parent receipt".to_string(),
            );
        }
        if receipt.source_range.first_parent_count != 1
            || receipt.swarm_range.first_parent_count != 1
        {
            return Err("first-parent denominator was not preserved".to_string());
        }
        if test_git_output(&source, &["rev-parse", "HEAD"])? != source_head_before
            || test_git_output(&swarm, &["rev-parse", "HEAD"])? != swarm_head_before
        {
            return Err("preflight moved an authoritative checkout".to_string());
        }
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn disposable_dry_merge_reports_clean_non_conflicting_history() -> Result<(), String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock before epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-preflight-clean-{nonce}"));
        let source = root.join("source");
        let swarm = root.join("swarm");
        fs::create_dir_all(&source).map_err(|error| error.to_string())?;
        test_git(&source, &["init", "--quiet"])?;
        test_git(&source, &["config", "user.email", "test@example.invalid"])?;
        test_git(&source, &["config", "user.name", "test"])?;
        fs::write(source.join("base.txt"), "base\n").map_err(|error| error.to_string())?;
        test_git(&source, &["add", "base.txt"])?;
        test_git(&source, &["commit", "--quiet", "-m", "base"])?;
        let base = test_git_output(&source, &["rev-parse", "HEAD"])?;
        test_git(&source, &["clone", ".", swarm.to_string_lossy().as_ref()])?;
        test_git(&source, &["checkout", "--quiet", "-B", "main", &base])?;
        test_git(&swarm, &["config", "user.email", "test@example.invalid"])?;
        test_git(&swarm, &["config", "user.name", "test"])?;
        fs::write(source.join("source.txt"), "source\n").map_err(|error| error.to_string())?;
        test_git(&source, &["add", "source.txt"])?;
        test_git(&source, &["commit", "--quiet", "-m", "source"])?;
        let source_parent = test_git_output(&source, &["rev-parse", "HEAD"])?;
        fs::write(source.join("reviewed.txt"), "reviewed\n").map_err(|error| error.to_string())?;
        test_git(&source, &["add", "reviewed.txt"])?;
        let reviewed_tree = test_git_output(&source, &["write-tree"])?;
        if verify_tree_object(&source, &reviewed_tree)? != true {
            return Err("write-tree did not create a source object-store tree".to_string());
        }
        fs::write(swarm.join("swarm.txt"), "swarm\n").map_err(|error| error.to_string())?;
        test_git(&swarm, &["add", "swarm.txt"])?;
        test_git(&swarm, &["commit", "--quiet", "-m", "swarm"])?;
        let swarm_parent = test_git_output(&swarm, &["rev-parse", "HEAD"])?;
        let reviewed_resolved_tree_verified =
            verify_tree_in_repositories(&source, &swarm, &reviewed_tree).is_ok();
        if !reviewed_resolved_tree_verified {
            return Err("reviewed write-tree object was not found".to_string());
        }
        let dry_merge =
            with_disposable_repo(&source, &swarm, &source_parent, &swarm_parent, |repo| {
                dry_merge_from_repo(
                    repo,
                    &source_parent,
                    &swarm_parent,
                    Some(&reviewed_tree),
                    reviewed_resolved_tree_verified,
                )
            })?;
        if dry_merge.status != "clean"
            || !dry_merge.conflicts.is_empty()
            || dry_merge.preview_tree.is_none()
            || dry_merge.reviewed_resolved_tree.as_deref() != Some(reviewed_tree.as_str())
            || !dry_merge.reviewed_resolved_tree_verified
        {
            return Err(format!(
                "non-conflicting pair was not reported cleanly: {dry_merge:?}"
            ));
        }
        fs::remove_dir_all(&root).map_err(|error| format!("cleanup failed: {error}"))?;
        Ok(())
    }

    fn test_git(repo: &Path, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .map_err(|error| format!("test git failed to start: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "test git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }

    fn test_git_output(repo: &Path, args: &[&str]) -> Result<String, String> {
        test_git(repo, args)?;
        let output = Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .map_err(|error| format!("test git failed to start: {error}"))?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
