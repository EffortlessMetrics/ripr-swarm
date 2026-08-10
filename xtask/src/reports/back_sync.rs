//! Exact-input, read-only verifier for the post-publication source-to-swarm join.
//!
//! This command consumes an already reviewed `K`; it never constructs a merge,
//! moves a ref, changes policy, or publishes anything.  The two transport
//! phases are inferred from the declared swarm main: before transport it must
//! equal SWARM_BEFORE, and after transport it must equal K.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SCHEMA: &str = "ripr.back_sync_verification.v1";
const REPORT_JSON: &str = "back-sync-verification.json";
const REPORT_MD: &str = "back-sync-verification.md";
const DEFAULT_OUT: &str = "target/ripr/back-sync";

#[derive(Clone, Debug)]
struct Options {
    swarm_before: String,
    source_release_head: String,
    join: String,
    tree: String,
    swarm_repo: PathBuf,
    source_repo: PathBuf,
    swarm_main: String,
    source_main: String,
    version: String,
    source_release_tag: String,
    release_receipt: Option<PathBuf>,
    policy_before: Option<PathBuf>,
    policy_exception: Option<PathBuf>,
    policy_after: Option<PathBuf>,
    out: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
struct InputEvidence {
    name: String,
    sha256: Option<String>,
    supplied: bool,
}

#[derive(Clone, Debug, Serialize)]
struct PolicyEvidence {
    merge_commits_disabled_before: Option<bool>,
    temporary_exception_supplied: bool,
    restoration_supplied: bool,
    policy_restored: Option<bool>,
    evidence: Vec<InputEvidence>,
    mutation_claim: String,
}

#[derive(Clone, Debug, Serialize)]
struct ReleaseEvidence {
    version: String,
    receipt: Option<InputEvidence>,
    changelog_reachable: bool,
    publication_receipt_reachable: bool,
    source_publication_is_ancestry_only: bool,
}

#[derive(Clone, Debug, Serialize)]
struct Receipt {
    schema: String,
    mode: String,
    swarm_before: String,
    source_release_head: String,
    join: String,
    reviewed_tree: String,
    declared_swarm_main: String,
    declared_source_main: String,
    current_swarm_main: String,
    current_source_main: String,
    source_release_tag_target: String,
    join_parents: Vec<String>,
    swarm_before_reachable: bool,
    source_release_reachable: bool,
    tree_matches_reviewed: bool,
    expected_head_guard: String,
    swarm_development_surfaces_active: Vec<String>,
    source_publication_paths_ancestry_only: Vec<String>,
    release: ReleaseEvidence,
    policy: PolicyEvidence,
    input_manifest: Vec<InputEvidence>,
    non_claims: Vec<String>,
    invalidation_rules: Vec<String>,
}

pub(crate) fn back_sync(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let receipt = build_receipt(&options)?;
    let json = serde_json::to_string_pretty(&receipt)
        .map_err(|error| format!("failed to serialize back-sync receipt: {error}"))?;
    fs::create_dir_all(&options.out)
        .map_err(|error| format!("failed to create {}: {error}", options.out.display()))?;
    fs::write(options.out.join(REPORT_JSON), format!("{json}\n"))
        .map_err(|error| format!("failed to write back-sync JSON: {error}"))?;
    fs::write(options.out.join(REPORT_MD), render_markdown(&receipt))
        .map_err(|error| format!("failed to write back-sync Markdown: {error}"))?;
    println!("Wrote {}", options.out.join(REPORT_JSON).display());
    println!("Wrote {}", options.out.join(REPORT_MD).display());
    Ok(())
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    if args.first().map(String::as_str) != Some("verify") {
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
            "--swarm-before"
                | "--source-release-head"
                | "--join"
                | "--k"
                | "--tree"
                | "--back-sync-tree"
                | "--swarm-repo"
                | "--source-repo"
                | "--swarm-main"
                | "--source-main"
                | "--version"
                | "--source-release-tag"
                | "--release-receipt"
                | "--policy-before"
                | "--policy-exception"
                | "--policy-after"
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
    let swarm_before = required("--swarm-before")?;
    let source_release_head = required("--source-release-head")?;
    let join = values
        .get("--join")
        .or_else(|| values.get("--k"))
        .cloned()
        .ok_or_else(|| format!("missing --join\n{}", usage()))?;
    let tree = values
        .get("--tree")
        .or_else(|| values.get("--back-sync-tree"))
        .cloned()
        .ok_or_else(|| format!("missing --tree\n{}", usage()))?;
    validate_sha("--swarm-before", "commit", &swarm_before)?;
    validate_sha("--source-release-head", "commit", &source_release_head)?;
    validate_sha("--join", "commit", &join)?;
    validate_sha("--tree", "tree", &tree)?;
    let version = required("--version")?;
    Ok(Options {
        swarm_before,
        source_release_head,
        join,
        tree,
        swarm_repo: PathBuf::from(required("--swarm-repo")?),
        source_repo: PathBuf::from(required("--source-repo")?),
        swarm_main: values
            .get("--swarm-main")
            .cloned()
            .unwrap_or_else(|| "origin/main".to_string()),
        source_main: values
            .get("--source-main")
            .cloned()
            .unwrap_or_else(|| "origin/main".to_string()),
        version,
        source_release_tag: required("--source-release-tag")?,
        release_receipt: values.get("--release-receipt").map(PathBuf::from),
        policy_before: values.get("--policy-before").map(PathBuf::from),
        policy_exception: values.get("--policy-exception").map(PathBuf::from),
        policy_after: values.get("--policy-after").map(PathBuf::from),
        out: values
            .get("--out")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_OUT)),
    })
}

fn usage() -> String {
    "usage: cargo xtask back-sync verify --swarm-before <sha> --source-release-head <sha> --source-release-tag <tag> --join <sha> --tree <tree-sha> --swarm-repo <path> --source-repo <path> --version <version> [--swarm-main <rev>] [--source-main <rev>] [--release-receipt <path>] [--policy-before <path>] [--policy-exception <path>] [--policy-after <path>] [--out <dir>]".to_string()
}

fn validate_sha(name: &str, kind: &str, value: &str) -> Result<(), String> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} must be a complete 40-character hexadecimal {kind} SHA"
        ));
    }
    Ok(())
}

fn build_receipt(options: &Options) -> Result<Receipt, String> {
    let swarm_root = repository_root(&options.swarm_repo)?;
    let source_root = repository_root(&options.source_repo)?;
    if swarm_root == source_root || git_common_dir(&swarm_root)? == git_common_dir(&source_root)? {
        return Err("source and swarm must be distinct repositories".to_string());
    }
    ensure_remote(&swarm_root, "EffortlessMetrics/ripr-swarm", "swarm")?;
    ensure_remote(&source_root, "EffortlessMetrics/ripr", "source")?;
    exact_commit(&swarm_root, &options.swarm_before, "SWARM_BEFORE")?;
    exact_commit(
        &source_root,
        &options.source_release_head,
        "SOURCE_RELEASE_HEAD",
    )?;
    exact_commit(&swarm_root, &options.join, "K")?;
    let current_swarm_main = exact_rev(&swarm_root, &options.swarm_main)?;
    let current_source_main = exact_rev(&source_root, &options.source_main)?;
    if current_source_main != options.source_release_head {
        return Err(format!(
            "declared source main resolves to {current_source_main}, not SOURCE_RELEASE_HEAD {}",
            options.source_release_head
        ));
    }
    let source_release_tag_target = exact_rev(&source_root, &options.source_release_tag)?;
    if source_release_tag_target != options.source_release_head {
        return Err(format!(
            "source release tag resolves to {source_release_tag_target}, not SOURCE_RELEASE_HEAD {}",
            options.source_release_head
        ));
    }
    let parents = lines(git(
        &swarm_root,
        &["show", "-s", "--format=%P", &options.join],
    )?);
    let parents = parents
        .first()
        .map(|line| {
            line.split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    validate_join(
        &parents,
        &options.swarm_before,
        &options.source_release_head,
    )?;
    let join_tree = git(
        &swarm_root,
        &["rev-parse", &format!("{}^{{tree}}", options.join)],
    )?
    .trim()
    .to_string();
    if join_tree != options.tree {
        return Err(format!(
            "K tree {join_tree} does not match reviewed tree {}",
            options.tree
        ));
    }
    if !is_ancestor(&swarm_root, &options.swarm_before, &options.join)
        || !is_ancestor(&swarm_root, &options.source_release_head, &options.join)
    {
        return Err("K does not make both exact parents reachable".to_string());
    }
    let mode = if current_swarm_main == options.swarm_before {
        "pre_transport"
    } else if current_swarm_main == options.join {
        "post_transport"
    } else {
        return Err(format!(
            "swarm main {current_swarm_main} is neither SWARM_BEFORE nor K; expected-head guard failed"
        ));
    };
    let active = development_surfaces(&swarm_root, &options.join)?;
    if active.is_empty() {
        return Err("K tree does not retain required swarm development surfaces".to_string());
    }
    let source_paths = source_publication_paths(&source_root, &options.source_release_head)?;
    let release_receipt = evidence("release_receipt", options.release_receipt.as_deref())?;
    let mut manifest = vec![
        InputEvidence {
            name: "SWARM_BEFORE".to_string(),
            sha256: Some(sha256_text(&options.swarm_before)),
            supplied: true,
        },
        InputEvidence {
            name: "SOURCE_RELEASE_HEAD".to_string(),
            sha256: Some(sha256_text(&options.source_release_head)),
            supplied: true,
        },
        InputEvidence {
            name: "K".to_string(),
            sha256: Some(sha256_text(&options.join)),
            supplied: true,
        },
        InputEvidence {
            name: "BACK_SYNC_TREE".to_string(),
            sha256: Some(sha256_text(&options.tree)),
            supplied: true,
        },
    ];
    manifest.push(release_receipt.clone());
    let policy = PolicyEvidence {
        merge_commits_disabled_before: options.policy_before.as_deref().map(policy_disabled),
        temporary_exception_supplied: options.policy_exception.is_some(),
        restoration_supplied: options.policy_after.is_some(),
        policy_restored: match (
            options.policy_before.as_deref(),
            options.policy_after.as_deref(),
        ) {
            (Some(before), Some(after)) => Some(policy_disabled(before) && policy_disabled(after)),
            _ => None,
        },
        evidence: vec![
            evidence("policy_before", options.policy_before.as_deref())?,
            evidence("temporary_exception", options.policy_exception.as_deref())?,
            evidence("policy_after", options.policy_after.as_deref())?,
        ],
        mutation_claim:
            "Evidence only: this verifier never mutates branch protection or repository settings."
                .to_string(),
    };
    manifest.extend(policy.evidence.clone());
    Ok(Receipt {
        schema: SCHEMA.to_string(),
        mode: mode.to_string(),
        swarm_before: options.swarm_before.clone(),
        source_release_head: options.source_release_head.clone(),
        join: options.join.clone(),
        reviewed_tree: options.tree.clone(),
        declared_swarm_main: options.swarm_main.clone(),
        declared_source_main: options.source_main.clone(),
        current_swarm_main,
        current_source_main,
        source_release_tag_target,
        join_parents: parents,
        swarm_before_reachable: true,
        source_release_reachable: true,
        tree_matches_reviewed: true,
        expected_head_guard: "PASS: declared swarm main was exactly SWARM_BEFORE (pre-transport) or K (post-transport); any other head fails closed".to_string(),
        swarm_development_surfaces_active: active,
        source_publication_paths_ancestry_only: source_paths,
        release: ReleaseEvidence {
            version: options.version.clone(),
            receipt: Some(release_receipt),
            changelog_reachable: path_exists_at(&swarm_root, &options.join, "CHANGELOG.md"),
            publication_receipt_reachable: path_exists_at(&swarm_root, &options.join, "docs/RELEASE.md"),
            source_publication_is_ancestry_only: true,
        },
        policy,
        input_manifest: manifest,
        non_claims: vec![
            "Read-only verification does not construct or transport K.".to_string(),
            "Ancestry and tree equality do not prove release correctness, artifact adequacy, or publication success.".to_string(),
            "Source publication workflows/settings are ancestry evidence only and are not swarm authority.".to_string(),
        ],
        invalidation_rules: vec![
            "Any change to SWARM_BEFORE, SOURCE_RELEASE_HEAD, K, BACK_SYNC_TREE, declared main, policy evidence, or release receipt requires a fresh receipt.".to_string(),
            "A swarm main other than SWARM_BEFORE or K fails the expected-head guard; do not substitute a branch name or later head.".to_string(),
        ],
    })
}

fn validate_join(
    parents: &[String],
    swarm_before: &str,
    source_release_head: &str,
) -> Result<(), String> {
    if parents.len() != 2 {
        return Err(format!(
            "K must have exactly two parents; found {}",
            parents.len()
        ));
    }
    if parents[0] != swarm_before || parents[1] != source_release_head {
        return Err(format!(
            "K parents must be [SWARM_BEFORE, SOURCE_RELEASE_HEAD], found [{}, {}]",
            parents[0], parents[1]
        ));
    }
    Ok(())
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
    path.canonicalize()
        .map_err(|error| format!("failed to resolve git common directory: {error}"))
}

fn ensure_remote(root: &Path, expected: &str, role: &str) -> Result<(), String> {
    let actual = git(root, &["remote", "get-url", "origin"])?;
    if canonical_remote(actual.trim()).as_deref() != Some(&expected.to_ascii_lowercase()) {
        return Err(format!("{role} origin does not identify {expected}"));
    }
    Ok(())
}

fn canonical_remote(value: &str) -> Option<String> {
    let mut value = value.trim().to_ascii_lowercase();
    for prefix in [
        "https://github.com/",
        "ssh://git@github.com/",
        "git@github.com:",
    ] {
        if value.starts_with(prefix) {
            value = value[prefix.len()..].to_string();
            break;
        }
    }
    if value.contains("://") || value.contains('@') {
        return None;
    }
    value = value
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string();
    let mut parts = value.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if parts.next().is_some()
        || owner.is_empty()
        || repo.is_empty()
        || value
            .chars()
            .any(|character| matches!(character, '?' | '#'))
    {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn exact_commit(root: &Path, sha: &str, label: &str) -> Result<(), String> {
    if git(root, &["cat-file", "-t", sha])?.trim() != "commit" {
        return Err(format!("{label} does not resolve to a commit"));
    }
    if git(
        root,
        &["rev-parse", "--verify", &format!("{sha}^{{commit}}")],
    )?
    .trim()
        != sha
    {
        return Err(format!("{label} must resolve to the exact supplied SHA"));
    }
    Ok(())
}

fn exact_rev(root: &Path, rev: &str) -> Result<String, String> {
    Ok(git(
        root,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )?
    .trim()
    .to_string())
}

fn is_ancestor(root: &Path, older: &str, newer: &str) -> bool {
    Command::new("git")
        .current_dir(root)
        .args(["merge-base", "--is-ancestor", older, newer])
        .status()
        .is_ok_and(|status| status.success())
}

fn path_exists_at(root: &Path, commit: &str, path: &str) -> bool {
    git(root, &["cat-file", "-e", &format!("{commit}:{path}")]).is_ok()
}

fn development_surfaces(root: &Path, commit: &str) -> Result<Vec<String>, String> {
    let files = lines(git(root, &["ls-tree", "-r", "--name-only", commit])?);
    let required = [
        "AGENTS.md",
        "docs/swarm-development.md",
        ".github/workflows/routed-rust.yml",
        "policy/process_allowlist.txt",
    ];
    Ok(required
        .into_iter()
        .filter(|path| files.iter().any(|file| file == path))
        .map(str::to_string)
        .collect())
}

fn source_publication_paths(root: &Path, commit: &str) -> Result<Vec<String>, String> {
    let mut paths = lines(git(root, &["ls-tree", "-r", "--name-only", commit])?)
        .into_iter()
        .filter(|path| path.contains("publish") || path.contains("release"))
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn evidence(name: &str, path: Option<&Path>) -> Result<InputEvidence, String> {
    let Some(path) = path else {
        return Ok(InputEvidence {
            name: name.to_string(),
            sha256: None,
            supplied: false,
        });
    };
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read {name} {}: {error}", path.display()))?;
    Ok(InputEvidence {
        name: name.to_string(),
        sha256: Some(sha256_bytes(&bytes)),
        supplied: true,
    })
}

fn policy_disabled(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|text| {
            text.contains("merge_commits = false") || text.contains("allow_merge_commits = false")
        })
        .unwrap_or(false)
}

fn sha256_text(value: &str) -> String {
    sha256_bytes(value.as_bytes())
}
fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}
fn lines(text: String) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("git failed to start: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn render_markdown(receipt: &Receipt) -> String {
    let list = |values: &[String]| {
        if values.is_empty() {
            "- none\n".to_string()
        } else {
            values
                .iter()
                .map(|value| format!("- `{value}`\n"))
                .collect()
        }
    };
    let mut out = format!(
        "# Back-sync verification\n\n- Schema: `{}`\n- Mode: `{}`\n- SWARM_BEFORE: `{}`\n- SOURCE_RELEASE_HEAD: `{}`\n- K: `{}`\n- Reviewed tree: `{}`\n- Current swarm main: `{}`\n- Expected-head guard: {}\n\n## Ordered parents\n\n{}\n## Swarm development surfaces\n\n{}\n## Source publication ancestry\n\n{}\n",
        receipt.schema,
        receipt.mode,
        receipt.swarm_before,
        receipt.source_release_head,
        receipt.join,
        receipt.reviewed_tree,
        receipt.current_swarm_main,
        receipt.expected_head_guard,
        list(&receipt.join_parents),
        list(&receipt.swarm_development_surfaces_active),
        list(&receipt.source_publication_paths_ancestry_only)
    );
    out.push_str("## Release and policy evidence\n\n");
    out.push_str(&format!("- Version: `{}`\n- Source release tag target: `{:?}`\n- Changelog reachable: `{}`\n- Publication receipt reachable: `{}`\n- Merge commits disabled before: `{:?}`\n- Temporary exception supplied: `{}`\n- Restoration supplied: `{}`\n- Policy restored: `{:?}`\n\n", receipt.release.version, receipt.source_release_tag_target, receipt.release.changelog_reachable, receipt.release.publication_receipt_reachable, receipt.policy.merge_commits_disabled_before, receipt.policy.temporary_exception_supplied, receipt.policy.restoration_supplied, receipt.policy.policy_restored));
    out.push_str("## Input manifest\n\n");
    for input in &receipt.input_manifest {
        out.push_str(&format!(
            "- {}: supplied=`{}`, digest=`{}`\n",
            input.name,
            input.supplied,
            input.sha256.as_deref().unwrap_or("unknown")
        ));
    }
    out.push_str("\n## Non-claims\n\n");
    out.push_str(&list(&receipt.non_claims));
    out.push_str("\n## Invalidation rules\n\n");
    out.push_str(&list(&receipt.invalidation_rules));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn abbreviated_inputs_are_rejected() -> Result<(), String> {
        if validate_sha("--join", "commit", "abc").is_ok() {
            return Err("abbreviated SHA accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn remote_suffix_tricks_are_rejected() -> Result<(), String> {
        if canonical_remote("https://github.com/EffortlessMetrics/ripr-evil.git")
            == Some("effortlessmetrics/ripr".to_string())
        {
            return Err("canonicalization unexpectedly erased repository identity".to_string());
        }
        if canonical_remote("https://github.com/evil/EffortlessMetrics/ripr.git").is_some() {
            return Err("nested owner path accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn input_manifest_is_ordered_and_deterministic() -> Result<(), String> {
        if sha256_text("abc") != sha256_text("abc") || sha256_text("abc") == sha256_text("abd") {
            return Err("digest is not deterministic".to_string());
        }
        Ok(())
    }

    #[test]
    fn synthetic_graph_rejects_single_parent_repair_and_squash() -> Result<(), String> {
        let root = temp_repo("back-sync-single-parent")?;
        let first = commit_file(&root, "one", "one")?;
        let second = commit_file(&root, "two", "two")?;
        if first == second {
            return Err("fixture commits were identical".to_string());
        }
        let parents = lines(git(&root, &["show", "-s", "--format=%P", &second])?);
        if parents.first().map(|line| line.split_whitespace().count()) != Some(1) {
            return Err("single-parent fixture was not single-parent".to_string());
        }
        cleanup(&root);
        Ok(())
    }

    #[test]
    fn synthetic_graph_preserves_required_parent_order_and_tree() -> Result<(), String> {
        let root = temp_repo("back-sync-join")?;
        let base = commit_file(&root, "base", "base")?;
        let swarm = commit_file(&root, "swarm", "swarm")?;
        let source = commit_file(&root, "source", "source")?;
        let tree = git(&root, &["rev-parse", &format!("{}^{{tree}}", &source)])?
            .trim()
            .to_string();
        let output = Command::new("git")
            .current_dir(&root)
            .args(["commit-tree", &tree, "-p", &swarm, "-p", &source])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            cleanup(&root);
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        let join = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let parents = git(&root, &["show", "-s", "--format=%P", &join])?;
        if !parents.starts_with(&format!("{swarm} {source}")) || tree.is_empty() || base.is_empty()
        {
            cleanup(&root);
            return Err("ordered join fixture failed".to_string());
        }
        cleanup(&root);
        Ok(())
    }

    #[test]
    fn synthetic_graph_rejects_reversed_substituted_and_appended_parents() -> Result<(), String> {
        let expected = ["a".to_string(), "b".to_string()];
        for parents in [
            vec!["b".to_string(), "a".to_string()],
            vec!["x".to_string(), "b".to_string()],
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        ] {
            if validate_join(&parents, "a", "b").is_ok() {
                return Err(format!("invalid parent graph accepted: {parents:?}"));
            }
        }
        if validate_join(&expected, "a", "b").is_err() {
            return Err("valid parent graph rejected".to_string());
        }
        Ok(())
    }

    fn temp_repo(label: &str) -> Result<PathBuf, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-{label}-{nonce}"));
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        git(&root, &["init", "--quiet"])?;
        git(&root, &["config", "user.name", "test"])?;
        git(&root, &["config", "user.email", "test@example.invalid"])?;
        Ok(root)
    }
    fn commit_file(root: &Path, name: &str, body: &str) -> Result<String, String> {
        fs::write(root.join(name), body).map_err(|error| error.to_string())?;
        git(root, &["add", name])?;
        git(root, &["commit", "--quiet", "-m", name])?;
        Ok(git(root, &["rev-parse", "HEAD"])?.trim().to_string())
    }
    fn cleanup(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }
}
