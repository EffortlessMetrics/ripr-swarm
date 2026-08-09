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
    source_repo: PathBuf,
    swarm_repo: PathBuf,
    source_main: String,
    swarm_main: String,
    version: String,
    out: PathBuf,
    source_remote: String,
    swarm_remote: String,
}

#[derive(Clone, Debug, Serialize)]
struct RepositoryIdentity {
    role: String,
    remote: String,
    expected_remote: String,
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
    first_parent_ordered_recipe: String,
}

#[derive(Clone, Debug, Serialize)]
struct VersionSide {
    workspace_version: Option<String>,
    crate_version: Option<String>,
    extension_version: Option<String>,
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
    merged_tree: Option<String>,
    conflicts: Vec<String>,
    diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct Receipt {
    schema: String,
    mode: String,
    source_parent: String,
    swarm_parent: String,
    source_main: String,
    swarm_main: String,
    merge_base: String,
    source_repository: RepositoryIdentity,
    swarm_repository: RepositoryIdentity,
    source_range: CommitRange,
    swarm_range: CommitRange,
    source_survivors: Vec<String>,
    swarm_only_paths: Vec<String>,
    swarm_exclusions: Vec<String>,
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
    validate_sha("--source-parent", &source_parent)?;
    validate_sha("--swarm-parent", &swarm_parent)?;
    let version = required("--version")?;
    let source_repo = PathBuf::from(required("--source-repo")?);
    let swarm_repo = PathBuf::from(required("--swarm-repo")?);
    Ok(Options {
        source_parent,
        swarm_parent,
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
    "usage: cargo xtask source-promotion preflight --source-parent <full-sha> --swarm-parent <full-sha> --source-repo <path> --swarm-repo <path> --version <version> [--source-main <rev>] [--swarm-main <rev>] [--source-remote <owner/repo>] [--swarm-remote <owner/repo>] [--out <dir>]".to_string()
}

fn validate_sha(name: &str, value: &str) -> Result<(), String> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} must be a complete 40-character hexadecimal commit SHA"
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
    )?;
    let swarm = inspect_repository(
        "swarm",
        &options.swarm_repo,
        &options.swarm_remote,
        &options.swarm_parent,
        &options.swarm_main,
    )?;
    let source_root = repository_root(&options.source_repo)?;
    let swarm_root = repository_root(&options.swarm_repo)?;
    if source_root == swarm_root {
        return Err(
            "source and swarm repository roots must be distinct; refusing an ambiguous identity"
                .to_string(),
        );
    }
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
                let dry_merge =
                    dry_merge_from_repo(repo, &options.source_parent, &options.swarm_parent)?;
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
    let mut swarm_exclusions = swarm_paths
        .iter()
        .filter(|path| is_swarm_authority_path(path))
        .cloned()
        .collect::<Vec<_>>();
    swarm_exclusions.sort();
    Ok(Receipt {
        schema: SCHEMA.to_string(),
        mode: if merge_base == options.source_parent {
            "fast_forward".to_string()
        } else {
            "two_parent_join".to_string()
        },
        source_parent: options.source_parent.clone(),
        swarm_parent: options.swarm_parent.clone(),
        source_main: source.resolved_main,
        swarm_main: swarm.resolved_main,
        merge_base,
        source_repository: source.identity,
        swarm_repository: swarm.identity,
        source_range,
        swarm_range,
        source_survivors: source_paths,
        swarm_only_paths: swarm_paths,
        swarm_exclusions,
        dry_merge,
        version_state: VersionState {
            requested_version: options.version.clone(),
            source: version_side(&source_root, &options.version),
            swarm: version_side(&swarm_root, &options.version),
        },
        invalidation_rules: vec![
            "Any change to SOURCE_PARENT, SWARM_PARENT, source main, or swarm main invalidates this receipt; regenerate rather than editing it.".to_string(),
            "A changed repository remote, merge base, ancestry count, ordered SHA digest, conflict list, or resolved tree requires a fresh receipt.".to_string(),
            "This receipt does not construct or prove the source join, version metadata, qualification, publication, or back-sync.".to_string(),
        ],
        next_commands: vec![
            "Review every dry-merge conflict and semantic overlap before constructing a join.".to_string(),
            "Pass this exact parent pair and receipt to source preflight; do not substitute a branch name or later main.".to_string(),
        ],
    })
}

struct InspectedRepository {
    identity: RepositoryIdentity,
    resolved_main: String,
}

fn inspect_repository(
    role: &str,
    repo: &Path,
    expected_remote: &str,
    parent: &str,
    main: &str,
) -> Result<InspectedRepository, String> {
    let root = repository_root(repo)?;
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
    Ok(InspectedRepository {
        identity: RepositoryIdentity {
            role: role.to_string(),
            remote,
            expected_remote: expected_remote.to_string(),
            identity: "origin-remote-and-git-root-verified".to_string(),
            root_verified: true,
            remote_verified: true,
        },
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

fn remote_matches(actual: &str, expected: &str) -> bool {
    let normalize = |value: &str| {
        value
            .trim_end_matches("/")
            .trim_end_matches(".git")
            .to_ascii_lowercase()
    };
    let actual = normalize(actual);
    let expected = normalize(expected)
        .trim_start_matches("https://github.com/")
        .trim_start_matches("git@github.com:")
        .to_string();
    actual.ends_with(&expected)
}

fn commit_range(repo: &Path, base: &str, head: &str) -> Result<CommitRange, String> {
    let all = lines(git(repo, &["rev-list", &format!("{base}..{head}")])?);
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
    let _ = fs::remove_dir_all(&path);
    result
}

fn dry_merge_from_repo(
    repo: &Path,
    source_parent: &str,
    swarm_parent: &str,
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
    let merged_tree = stdout
        .lines()
        .next()
        .filter(|line| line.len() == 40)
        .map(str::to_string);
    Ok(DryMerge {
        status: if conflicts.is_empty() {
            "clean"
        } else {
            "conflicts"
        }
        .to_string(),
        merged_tree,
        conflicts,
        diagnostics,
    })
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

fn version_side(root: &Path, version: &str) -> VersionSide {
    let read = |relative: &str| fs::read_to_string(root.join(relative)).ok();
    let cargo = read("Cargo.toml");
    let crate_manifest = read("crates/ripr/Cargo.toml");
    let package = read("editors/vscode/package.json");
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
    VersionSide {
        workspace_version: parse_version(cargo),
        crate_version: parse_version(crate_manifest),
        extension_version,
        changelog_mentions_version: changelog.contains(&format!("[{version}]"))
            || changelog.contains(&format!("## {version}")),
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
        "# Source-promotion preflight\n\n- Schema: `{}`\n- Mode: `{}`\n- SOURCE_PARENT: `{}`\n- SWARM_PARENT: `{}`\n- MERGE_BASE: `{}`\n\n## Repository identity\n\n- source: `{}` (expected `{}`)\n- swarm: `{}` (expected `{}`)\n\n## Ancestry\n\n| range | all reachable | first parent | all digest | ordered first-parent digest |\n| --- | ---: | ---: | --- | --- |\n| source | {} | {} | `{}` | `{}` |\n| swarm | {} | {} | `{}` | `{}` |\n\nOrdered digest recipe: `{}`\n\n## Dry merge\n\nStatus: **{}**\n\nConflicts:\n{}\n\n## Source survivors\n\n{}\n## Swarm-only paths\n\n{}\n## Swarm authority paths requiring explicit resolution\n\n{}\n## Version state\n\n- requested: `{}`\n- source: workspace=`{:?}`, crate=`{:?}`, extension=`{:?}`, changelog-mentions=`{}`\n- swarm: workspace=`{:?}`, crate=`{:?}`, extension=`{:?}`, changelog-mentions=`{}`\n\n## Invalidation rules\n\n{}\n## Next commands\n\n{}",
        receipt.schema,
        receipt.mode,
        receipt.source_parent,
        receipt.swarm_parent,
        receipt.merge_base,
        receipt.source_repository.remote,
        receipt.source_repository.expected_remote,
        receipt.swarm_repository.remote,
        receipt.swarm_repository.expected_remote,
        receipt.source_range.all_reachable_count,
        receipt.source_range.first_parent_count,
        receipt.source_range.all_reachable_sha256,
        receipt.source_range.first_parent_ordered_sha256,
        receipt.swarm_range.all_reachable_count,
        receipt.swarm_range.first_parent_count,
        receipt.swarm_range.all_reachable_sha256,
        receipt.swarm_range.first_parent_ordered_sha256,
        receipt.source_range.first_parent_ordered_recipe,
        receipt.dry_merge.status,
        list(&receipt.dry_merge.conflicts),
        list(&receipt.source_survivors),
        list(&receipt.swarm_only_paths),
        list(&receipt.swarm_exclusions),
        receipt.version_state.requested_version,
        receipt.version_state.source.workspace_version,
        receipt.version_state.source.crate_version,
        receipt.version_state.source.extension_version,
        receipt.version_state.source.changelog_mentions_version,
        receipt.version_state.swarm.workspace_version,
        receipt.version_state.swarm.crate_version,
        receipt.version_state.swarm.extension_version,
        receipt.version_state.swarm.changelog_mentions_version,
        list(&receipt.invalidation_rules),
        list(&receipt.next_commands),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
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
    fn digest_recipe_is_order_sensitive() -> Result<(), String> {
        let left = digest_lines(&["a".to_string(), "b".to_string()]);
        let right = digest_lines(&["b".to_string(), "a".to_string()]);
        if left == right {
            return Err("ordered digest ignored input order".to_string());
        }
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
        let receipt = build_receipt(&Options {
            source_parent,
            swarm_parent,
            source_repo: source.clone(),
            swarm_repo: swarm.clone(),
            source_main: "HEAD".to_string(),
            swarm_main: "HEAD".to_string(),
            version: "0.11.0".to_string(),
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

    #[allow(dead_code)]
    fn _os_string_is_available_for_windows_tests() -> OsString {
        OsString::from("git")
    }
}
