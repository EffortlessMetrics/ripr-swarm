//! Read-only current-state inventory producer and drift checker (#2330, consumed
//! by #2331 cold-start orientation). Reports observable repository facts only;
//! it does not decide support tiers, close issues, or authorize releases.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub(crate) const CURRENT_STATE_SCHEMA_VERSION: &str = "repository_current_state.v1";
pub(crate) const CURRENT_STATE_CLAIM_BOUNDARY: &str = "Read-only current-state inventory: \
 observable repository facts from authoritative files and the command catalog; it does not \
 decide whether a support tier is deserved, whether a spec is complete, whether an issue \
 should close, or whether a release is authorized.";

/// Where ordinary development happens, and the source release umbrella. These
/// name the program's authorities, not the checkout the command runs in. The
/// orientation proof derives its receipt authorities from them, not from the
/// corpus it judges (#13074).
pub(crate) const DEVELOPMENT_AUTHORITY: &str = "EffortlessMetrics/ripr-swarm";
pub(crate) const RELEASE_AUTHORITY: &str = "EffortlessMetrics/ripr#1463";

/// Issue number of the release umbrella named by [`RELEASE_AUTHORITY`].
pub(crate) fn release_authority_issue() -> Option<u64> {
    RELEASE_AUTHORITY.rsplit('#').next()?.parse().ok()
}

const ROADMAP_PATH: &str = "docs/ROADMAP.md";
const DELETED_GOAL_MARKERS: [&str; 4] = [
    ".ripr/goals",
    "cargo xtask goals",
    "pr-body --work-item",
    "closeout --goal",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CurrentStateInventoryV1 {
    pub schema_version: String,
    pub repository: String,
    pub default_branch: String,
    pub crate_version: String,
    pub rust_msrv: String,
    pub development_authority: String,
    pub release_authority: String,
    pub command_catalog_entries: u64,
    pub live_doc_deleted_command_hits: Vec<String>,
    pub registered_artifacts_by_kind_and_status: BTreeMap<String, BTreeMap<String, u64>>,
    pub support_rows_by_tier: BTreeMap<String, u64>,
    pub contract_violations: Vec<String>,
    pub coverage_limitations: Vec<String>,
    pub claim_boundary: String,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read_roadmap_current_section(root: &Path) -> Result<String, String> {
    let body = fs::read_to_string(root.join(ROADMAP_PATH))
        .map_err(|error| format!("read {ROADMAP_PATH}: {error}"))?;
    let historical = body
        .find("\n## Historical roadmap and campaign archive\n")
        .unwrap_or(body.len());
    Ok(body[..historical].to_string())
}

fn scan_deleted_commands(section: &str) -> Vec<String> {
    let mut hits = BTreeSet::new();
    for marker in DELETED_GOAL_MARKERS {
        if section.contains(marker) {
            hits.insert(marker.to_string());
        }
    }
    hits.into_iter().collect()
}

/// Value of `key = "..."` inside the `[workspace.package]` table only, so an
/// earlier `version = ` in another table (a dependency, a profile) is never read.
fn workspace_package_value(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} = ");
    let mut in_table = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_table = trimmed == "[workspace.package]";
            continue;
        }
        if in_table && let Some(rest) = trimmed.strip_prefix(prefix.as_str()) {
            let value = rest.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn read_workspace_manifest(root: &Path) -> Result<String, String> {
    fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| format!("read workspace Cargo.toml: {error}"))
}

fn read_crate_version(root: &Path) -> Result<String, String> {
    workspace_package_value(&read_workspace_manifest(root)?, "version")
        .ok_or_else(|| "workspace Cargo.toml is missing [workspace.package] version".to_string())
}

fn read_msrv(root: &Path) -> Result<String, String> {
    workspace_package_value(&read_workspace_manifest(root)?, "rust-version").ok_or_else(|| {
        "workspace Cargo.toml is missing [workspace.package] rust-version (MSRV)".to_string()
    })
}

/// `OWNER/REPO` for a GitHub remote URL in the https, scp-like ssh or ssh://
/// spellings; `None` for anything else (never a guess).
fn github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.strip_prefix("git@github.com:"))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (valid(owner) && valid(repo)).then(|| format!("{owner}/{repo}"))
}

/// The repository this checkout is, from its configured `origin` (read from the
/// local config only, so an `insteadOf` rewrite cannot change it). The inventory
/// describes the repository it runs in; a constant would go stale in the source
/// repository after the step-2 sync (#13049).
fn read_repository(root: &Path) -> Result<String, String> {
    let args: Vec<String> = [
        "config",
        "--local",
        "--no-includes",
        "--get-all",
        "remote.origin.url",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    let output = crate::run::capture_process_output_in("git", &args, Some(root), &[], &[], &[])
        .map_err(|error| format!("read remote.origin.url: {}", error.message))?;
    let text = String::from_utf8_lossy(&output);
    if text.lines().count() != 1 {
        return Err(
            "remote.origin.url must have exactly one value to name the repository".to_string(),
        );
    }
    github_slug(&text).ok_or_else(|| {
        format!(
            "remote.origin.url `{}` is not a recognized GitHub remote",
            text.trim()
        )
    })
}

pub(crate) fn produce_current_state(root: &Path) -> Result<CurrentStateInventoryV1, String> {
    let current_section = read_roadmap_current_section(root)?;
    let catalog_count = crate::command::command_catalog().len() as u64;
    let summary = crate::repo_contract_summary(root)?;
    let mut registered_artifacts_by_kind_and_status = BTreeMap::new();
    for artifact in &summary.artifacts {
        let statuses = registered_artifacts_by_kind_and_status
            .entry(artifact.kind.clone())
            .or_insert_with(BTreeMap::new);
        *statuses.entry(artifact.status.clone()).or_insert(0) += 1;
    }
    let mut support_rows_by_tier = BTreeMap::new();
    for row in &summary.support_rows {
        *support_rows_by_tier
            .entry(crate::normalized_support_tier(&row.tier))
            .or_insert(0) += 1;
    }
    // Existing fragment diagnostics include their input path. Keep report bytes
    // independent of the absolute checkout root, preserving the named diagnostic.
    let root_prefix = format!("{}/", crate::normalize_path(root).trim_end_matches('/'));
    let contract_violations = summary
        .missing_links
        .iter()
        .map(|violation| violation.replace(&root_prefix, ""))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(CurrentStateInventoryV1 {
        schema_version: CURRENT_STATE_SCHEMA_VERSION.to_string(),
        repository: read_repository(root)?,
        default_branch: "main".to_string(),
        crate_version: read_crate_version(root)?,
        rust_msrv: read_msrv(root)?,
        development_authority: DEVELOPMENT_AUTHORITY.to_string(),
        release_authority: RELEASE_AUTHORITY.to_string(),
        command_catalog_entries: catalog_count,
        live_doc_deleted_command_hits: scan_deleted_commands(&current_section),
        registered_artifacts_by_kind_and_status,
        support_rows_by_tier,
        contract_violations,
        coverage_limitations: coverage_limitations(),
        claim_boundary: CURRENT_STATE_CLAIM_BOUNDARY.to_string(),
    })
}

pub(crate) fn check_current_state(root: &Path) -> Result<(), String> {
    let inventory = produce_current_state(root)?;
    if !inventory.live_doc_deleted_command_hits.is_empty() {
        return Err(format!(
            "current roadmap sections still name deleted execution authority: {:?}",
            inventory.live_doc_deleted_command_hits
        ));
    }
    if !inventory.contract_violations.is_empty() {
        return Err(format!(
            "current-state registered artifact/support contract violations:\n{}",
            inventory.contract_violations.join("\n")
        ));
    }
    if inventory.command_catalog_entries == 0 {
        return Err("command catalog is empty".to_string());
    }
    Ok(())
}

fn coverage_limitations() -> Vec<String> {
    [
        "Artifact counts cover registered ledger/fragment records only; unregistered filesystem artifacts are not counted, and recorded status is not semantic completeness.",
        "Support counts and violations reuse the existing structural checker; command existence does not establish earned support or executed proof.",
        "The default branch is the declared main convention, not a discovered remote setting.",
        "Authority declarations are program constants; authored authority contradictions are not checked.",
        "Command inventory is a catalog entry count; aliases and general live-document references are not inventoried. Deleted-reference checks cover only four current-roadmap markers.",
        "Release posture, implementation slices, fixtures/goldens/corpora and capability status are not inventoried.",
        "Module health/ownership, CI/protected-lane settings, language adapters/features/tiers and generated/traceability health are not inventoried; unrun checks have no health result.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn current_state_markdown(inventory: &CurrentStateInventoryV1) -> String {
    let mut body = format!(
        "# Current-state inventory\n\n{}\n\n## Repository\n\n\
         | Fact | Value |\n| --- | --- |\n\
         | Repository | {} |\n| Default branch (declared) | {} |\n\
         | Workspace version | {} |\n| Rust MSRV | {} |\n\
         | Development authority | {} |\n| Release authority | {} |\n\
         | Command catalog entries | {} |\n\n\
         ## Registered artifacts\n\nCounts describe registered records and recorded statuses.\n\n\
         | Kind | Recorded status | Count |\n| --- | --- | ---: |\n",
        inventory.claim_boundary,
        markdown_cell(&inventory.repository),
        markdown_cell(&inventory.default_branch),
        markdown_cell(&inventory.crate_version),
        markdown_cell(&inventory.rust_msrv),
        markdown_cell(&inventory.development_authority),
        markdown_cell(&inventory.release_authority),
        inventory.command_catalog_entries,
    );
    for (kind, statuses) in &inventory.registered_artifacts_by_kind_and_status {
        for (status, count) in statuses {
            body.push_str(&format!(
                "| {} | {} | {count} |\n",
                markdown_cell(kind),
                markdown_cell(status),
            ));
        }
    }
    body.push_str("\n## Support rows\n\n| Recorded tier | Count |\n| --- | ---: |\n");
    for (tier, count) in &inventory.support_rows_by_tier {
        body.push_str(&format!("| {} | {count} |\n", markdown_cell(tier)));
    }
    body.push_str("\n## Existing contract violations\n\n");
    if inventory.contract_violations.is_empty() {
        body.push_str("No violations reported by the existing artifact/support collectors.\n");
    } else {
        for violation in &inventory.contract_violations {
            body.push_str(&format!("- {}\n", markdown_cell(violation)));
        }
    }
    body.push_str("\n## Current-roadmap deleted references\n\n");
    if inventory.live_doc_deleted_command_hits.is_empty() {
        body.push_str("None of the four retired authority markers found in the current section.\n");
    } else {
        for reference in &inventory.live_doc_deleted_command_hits {
            body.push_str(&format!("- {}\n", markdown_cell(reference)));
        }
    }
    body.push_str("\n## Coverage and limitations\n\n");
    for limitation in &inventory.coverage_limitations {
        body.push_str(&format!("- {limitation}\n"));
    }
    body
}

fn write_current_state_reports(root: &Path, reports: &Path) -> Result<String, String> {
    let inventory = produce_current_state(root)?;
    let json_body = serde_json::to_string_pretty(&inventory)
        .map_err(|error| format!("serialize current-state inventory: {error}"))?;
    fs::create_dir_all(reports)
        .map_err(|error| format!("create current-state report directory: {error}"))?;
    fs::write(reports.join("current-state.json"), &json_body)
        .map_err(|error| format!("write current-state.json: {error}"))?;
    fs::write(
        reports.join("current-state.md"),
        current_state_markdown(&inventory),
    )
    .map_err(|error| format!("write current-state.md: {error}"))?;
    Ok(json_body)
}

pub(crate) fn current_state_command(args: &[String]) -> Result<(), String> {
    let mut check = false;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            "--json" => json = true,
            "--help" | "-h" => {
                return Err("usage: cargo xtask current-state [--check] [--json]".to_string());
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    let root = workspace_root();
    if check {
        check_current_state(&root)?;
        if json {
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "schema_version": "repository_current_state_check.v1",
                    "status": "pass",
                }))
                .map_err(|error| format!("serialize current-state check: {error}"))?
            );
        } else {
            println!("current-state check: pass");
        }
        return Ok(());
    }
    let json_body = write_current_state_reports(&root, &crate::reports_dir())?;
    if json {
        print!("{json_body}");
    } else {
        println!("wrote target/ripr/reports/current-state.json");
    }
    Ok(())
}

/// Snapshot the added producer's file inputs for read-only consumer proof.
/// Reuse the ledger/parser and support parser for dynamic paths. Directory
/// walks retain membership changes as well as content changes, without taking
/// a whole-workspace snapshot that races unrelated target/report writers.
#[cfg(test)]
pub(crate) type CurrentStateInputSnapshot = Vec<(String, Option<Vec<u8>>)>;

#[cfg(test)]
pub(crate) fn current_state_input_snapshot(root: &Path) -> Result<CurrentStateInputSnapshot, String> {
    let mut paths = BTreeSet::from([
        "docs/ROADMAP.md".to_string(),
        "Cargo.toml".to_string(),
        crate::SUPPORT_TIERS_PATH.to_string(),
        crate::DOC_ARTIFACT_LEDGER.to_string(),
        "README.md".to_string(),
        crate::RUST_REPAIR_TRUST_CORPUS_PATH.to_string(),
    ]);
    for directory in ["policy", "docs/specs"] {
        for path in crate::collect_files(&root.join(directory))? {
            paths.insert(crate::display_repo_path(root, &path));
        }
    }
    let ledger = crate::parse_doc_artifact_ledger(&root.join(crate::DOC_ARTIFACT_LEDGER))?;
    for artifact in ledger.artifacts {
        if let Some(path) = artifact.path
            && crate::doc_artifact_path_safety_violation(&path).is_none()
        {
            paths.insert(path);
        }
    }
    let support_path = root.join(crate::SUPPORT_TIERS_PATH);
    let support = fs::read_to_string(&support_path)
        .map_err(|error| format!("snapshot support tiers: {error}"))?;
    for row in crate::support_tier_rows(&support, crate::SUPPORT_TIERS_PATH)? {
        for reference in crate::inline_code_spans(&row.proof) {
            if (reference.starts_with("scripts/") || reference.starts_with(".github/workflows/"))
                && crate::doc_artifact_path_safety_violation(&reference).is_none()
            {
                paths.insert(reference);
            }
        }
    }
    paths
        .into_iter()
        .map(|relative| match fs::read(root.join(&relative)) {
            Ok(bytes) => Ok((relative, Some(bytes))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((relative, None)),
            Err(error) => Err(format!("snapshot {relative}: {error}")),
        })
        .collect()
}

#[cfg(test)]
mod contract_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_state_producer_is_deterministic_for_equal_inputs() -> Result<(), String> {
        let root = workspace_root();
        let first = produce_current_state(&root)?;
        let second = produce_current_state(&root)?;
        let first_json = serde_json::to_string(&first)
            .map_err(|error| format!("serialize first inventory: {error}"))?;
        let second_json = serde_json::to_string(&second)
            .map_err(|error| format!("serialize second inventory: {error}"))?;
        if first_json != second_json {
            return Err("current-state inventory must be deterministic".to_string());
        }
        Ok(())
    }

    #[test]
    fn current_state_check_passes_on_committed_roadmap() -> Result<(), String> {
        check_current_state(&workspace_root())
    }

    #[test]
    fn workspace_package_value_ignores_version_lines_in_other_tables() {
        let body = "[workspace.dependencies]\nserde = { version = \"1\" }\nversion = \"9.9.9\"\n\n\
                    [workspace.package]\nversion = \"0.11.0\"\nrust-version = \"1.95\"\n\n\
                    [profile.dev]\nversion = \"7\"\n";
        assert_eq!(
            workspace_package_value(body, "version").as_deref(),
            Some("0.11.0")
        );
        assert_eq!(
            workspace_package_value(body, "rust-version").as_deref(),
            Some("1.95")
        );
    }

    #[test]
    fn workspace_package_value_is_none_without_the_table() {
        let body = "[package]\nversion = \"1.0.0\"\n";
        assert_eq!(workspace_package_value(body, "version"), None);
    }

    #[test]
    fn github_slug_reads_each_remote_spelling() {
        for url in [
            "https://github.com/EffortlessMetrics/ripr-swarm",
            "https://github.com/EffortlessMetrics/ripr-swarm.git",
            "https://github.com/EffortlessMetrics/ripr-swarm/",
            "git@github.com:EffortlessMetrics/ripr-swarm.git",
            "ssh://git@github.com/EffortlessMetrics/ripr-swarm.git",
        ] {
            assert_eq!(
                github_slug(url).as_deref(),
                Some("EffortlessMetrics/ripr-swarm"),
                "{url}"
            );
        }
        assert_eq!(
            github_slug("https://github.com/EffortlessMetrics/ripr").as_deref(),
            Some("EffortlessMetrics/ripr")
        );
    }

    #[test]
    fn github_slug_rejects_foreign_and_malformed_remotes() {
        for url in [
            "https://example.com/EffortlessMetrics/ripr",
            "https://github.com/EffortlessMetrics",
            "https://github.com//ripr",
            "https://github.com/a/b/c",
            "/local/path/ripr",
            "",
        ] {
            assert_eq!(github_slug(url), None, "{url}");
        }
    }

    #[test]
    fn repository_follows_the_checkout_origin() -> Result<(), String> {
        let dir =
            std::env::temp_dir().join(format!("ripr-current-state-origin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
        let git = |args: &[&str]| -> Result<(), String> {
            let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
            crate::run::capture_process_output_in("git", &owned, Some(&dir), &[], &[], &[])
                .map(|_| ())
                .map_err(|error| error.message)
        };
        git(&["init", "-q"])?;
        git(&[
            "remote",
            "add",
            "origin",
            "https://github.com/EffortlessMetrics/ripr.git",
        ])?;
        let source = read_repository(&dir);
        git(&[
            "remote",
            "set-url",
            "origin",
            "git@github.com:Other/Fork.git",
        ])?;
        let fork = read_repository(&dir);
        fs::remove_dir_all(&dir).map_err(|error| format!("remove {}: {error}", dir.display()))?;
        assert_eq!(source?, "EffortlessMetrics/ripr");
        assert_eq!(fork?, "Other/Fork");
        Ok(())
    }
}
