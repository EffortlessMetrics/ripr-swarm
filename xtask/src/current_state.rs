//! Read-only current-state inventory producer and drift checker (#2330, consumed
//! by #2331 cold-start orientation). Reports observable repository facts only;
//! it does not decide support tiers, close issues, or authorize releases.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub(crate) const CURRENT_STATE_SCHEMA_VERSION: &str = "repository_current_state.v1";
pub(crate) const CURRENT_STATE_CLAIM_BOUNDARY: &str = "Read-only current-state inventory: \
 observable repository facts from authoritative files and the command catalog; it does not \
 decide whether a support tier is deserved, whether a spec is complete, whether an issue \
 should close, or whether a release is authorized.";

const ROADMAP_PATH: &str = "docs/ROADMAP.md";
const SUPPORT_TIERS_PATH: &str = "docs/status/SUPPORT_TIERS.md";
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

pub(crate) fn produce_current_state(root: &Path) -> Result<CurrentStateInventoryV1, String> {
    let current_section = read_roadmap_current_section(root)?;
    let catalog_count = crate::command::command_catalog().len() as u64;
    Ok(CurrentStateInventoryV1 {
        schema_version: CURRENT_STATE_SCHEMA_VERSION.to_string(),
        repository: "EffortlessMetrics/ripr-swarm".to_string(),
        default_branch: "main".to_string(),
        crate_version: read_crate_version(root)?,
        rust_msrv: read_msrv(root)?,
        development_authority: "EffortlessMetrics/ripr-swarm".to_string(),
        release_authority: "EffortlessMetrics/ripr#1463".to_string(),
        command_catalog_entries: catalog_count,
        live_doc_deleted_command_hits: scan_deleted_commands(&current_section),
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
    if !root.join(SUPPORT_TIERS_PATH).is_file() {
        return Err(format!(
            "missing support tier authority at {SUPPORT_TIERS_PATH}"
        ));
    }
    if inventory.command_catalog_entries == 0 {
        return Err("command catalog is empty".to_string());
    }
    Ok(())
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
    let inventory = produce_current_state(&root)?;
    let json_body = serde_json::to_string_pretty(&inventory)
        .map_err(|error| format!("serialize current-state inventory: {error}"))?;
    crate::write_report("current-state.json", &json_body)?;
    crate::write_report(
        "current-state.md",
        &format!("# Current-state inventory\n\n{}\n", json_body),
    )?;
    if json {
        print!("{json_body}");
    } else {
        println!("wrote target/ripr/reports/current-state.json");
    }
    Ok(())
}

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
}
