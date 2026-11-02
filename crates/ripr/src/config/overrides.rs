//! Per-crate / per-path `[[overrides]]` for severity and oracle policy (#2671).
//!
//! Repo-wide `[severity]` and `[oracles]` remain the default. An override
//! matches a Cargo package name or a repository-relative path glob; the most
//! specific match wins. Unknown `crate` names fail closed with a config
//! diagnostic once workspace packages are known.

use super::model::{
    ConfigSeverity, FindingSeverityConfig, OraclePolicy, RiprConfig, SeamSeverityConfig,
    SeverityConfig,
};
use crate::analysis::path_glob::path_glob_matches;
use crate::analysis::seams::SeamGripClass;
use crate::domain::{ExposureClass, OracleStrength};
use std::fs;
use std::path::Path;

/// How one `[[overrides]]` row selects the crate or path it applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OverrideSelector {
    Crate(String),
    Path(String),
}

impl OverrideSelector {
    fn identity_key(&self) -> String {
        match self {
            Self::Crate(name) => format!("crate:{name}"),
            Self::Path(pattern) => format!("path:{pattern}"),
        }
    }
}

/// One optional `[[overrides]]` row. Unspecified nested fields keep the
/// repo-wide value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfigOverride {
    pub selector: OverrideSelector,
    pub severity: Option<PartialSeverityConfig>,
    pub oracles: Option<PartialOraclePolicy>,
    /// After [`RiprConfig::bind_workspace_crates`], a crate-name selector
    /// also carries `{package_dir}/**` so path-only consumers can match.
    pub resolved_path_glob: Option<String>,
}

/// Workspace package identity used to resolve `crate = "..."` selectors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceCrate {
    pub name: String,
    /// Repository-relative package directory (`""` for a root package).
    pub dir: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PartialSeverityConfig {
    pub findings: PartialFindingSeverityConfig,
    pub seams: PartialSeamSeverityConfig,
}

impl PartialSeverityConfig {
    pub(crate) fn is_empty(&self) -> bool {
        self.findings.is_empty() && self.seams.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PartialFindingSeverityConfig {
    pub exposed: Option<ConfigSeverity>,
    pub weakly_exposed: Option<ConfigSeverity>,
    pub reachable_unrevealed: Option<ConfigSeverity>,
    pub no_static_path: Option<ConfigSeverity>,
    pub infection_unknown: Option<ConfigSeverity>,
    pub propagation_unknown: Option<ConfigSeverity>,
    pub static_unknown: Option<ConfigSeverity>,
}

impl PartialFindingSeverityConfig {
    fn is_empty(&self) -> bool {
        self.exposed.is_none()
            && self.weakly_exposed.is_none()
            && self.reachable_unrevealed.is_none()
            && self.no_static_path.is_none()
            && self.infection_unknown.is_none()
            && self.propagation_unknown.is_none()
            && self.static_unknown.is_none()
    }

    fn apply(&self, current: &mut FindingSeverityConfig) {
        assign_optional(&mut current.exposed, self.exposed);
        assign_optional(&mut current.weakly_exposed, self.weakly_exposed);
        assign_optional(&mut current.reachable_unrevealed, self.reachable_unrevealed);
        assign_optional(&mut current.no_static_path, self.no_static_path);
        assign_optional(&mut current.infection_unknown, self.infection_unknown);
        assign_optional(&mut current.propagation_unknown, self.propagation_unknown);
        assign_optional(&mut current.static_unknown, self.static_unknown);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PartialSeamSeverityConfig {
    pub strongly_gripped: Option<ConfigSeverity>,
    pub weakly_gripped: Option<ConfigSeverity>,
    pub ungripped: Option<ConfigSeverity>,
    pub reachable_unrevealed: Option<ConfigSeverity>,
    pub activation_unknown: Option<ConfigSeverity>,
    pub propagation_unknown: Option<ConfigSeverity>,
    pub observation_unknown: Option<ConfigSeverity>,
    pub discrimination_unknown: Option<ConfigSeverity>,
    pub opaque: Option<ConfigSeverity>,
    pub intentional: Option<ConfigSeverity>,
    pub suppressed: Option<ConfigSeverity>,
}

impl PartialSeamSeverityConfig {
    fn is_empty(&self) -> bool {
        self.strongly_gripped.is_none()
            && self.weakly_gripped.is_none()
            && self.ungripped.is_none()
            && self.reachable_unrevealed.is_none()
            && self.activation_unknown.is_none()
            && self.propagation_unknown.is_none()
            && self.observation_unknown.is_none()
            && self.discrimination_unknown.is_none()
            && self.opaque.is_none()
            && self.intentional.is_none()
            && self.suppressed.is_none()
    }

    fn apply(&self, current: &mut SeamSeverityConfig) {
        assign_optional(&mut current.strongly_gripped, self.strongly_gripped);
        assign_optional(&mut current.weakly_gripped, self.weakly_gripped);
        assign_optional(&mut current.ungripped, self.ungripped);
        assign_optional(&mut current.reachable_unrevealed, self.reachable_unrevealed);
        assign_optional(&mut current.activation_unknown, self.activation_unknown);
        assign_optional(&mut current.propagation_unknown, self.propagation_unknown);
        assign_optional(&mut current.observation_unknown, self.observation_unknown);
        assign_optional(
            &mut current.discrimination_unknown,
            self.discrimination_unknown,
        );
        assign_optional(&mut current.opaque, self.opaque);
        assign_optional(&mut current.intentional, self.intentional);
        assign_optional(&mut current.suppressed, self.suppressed);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PartialOraclePolicy {
    pub snapshot_strength: Option<OracleStrength>,
    pub mock_expectation_strength: Option<OracleStrength>,
    pub broad_error_strength: Option<OracleStrength>,
}

impl PartialOraclePolicy {
    pub(crate) fn is_empty(&self) -> bool {
        self.snapshot_strength.is_none()
            && self.mock_expectation_strength.is_none()
            && self.broad_error_strength.is_none()
    }

    fn apply(&self, current: &mut OraclePolicy) {
        if let Some(value) = self.snapshot_strength.clone() {
            current.snapshot_strength = value;
        }
        if let Some(value) = self.mock_expectation_strength.clone() {
            current.mock_expectation_strength = value;
        }
        if let Some(value) = self.broad_error_strength.clone() {
            current.broad_error_strength = value;
        }
    }

    fn identity_value(&self) -> String {
        let mut parts = Vec::new();
        if let Some(value) = &self.snapshot_strength {
            parts.push(format!("snapshot={}", value.as_str()));
        }
        if let Some(value) = &self.mock_expectation_strength {
            parts.push(format!("mock={}", value.as_str()));
        }
        if let Some(value) = &self.broad_error_strength {
            parts.push(format!("broad={}", value.as_str()));
        }
        parts.join(",")
    }
}

fn assign_optional<T: Copy>(slot: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *slot = value;
    }
}

impl ConfigOverride {
    fn matches(&self, crate_name: Option<&str>, path: &str) -> bool {
        match &self.selector {
            OverrideSelector::Crate(name) => {
                if crate_name == Some(name.as_str()) {
                    return true;
                }
                self.resolved_path_glob
                    .as_deref()
                    .is_some_and(|glob| path_glob_matches(glob, path))
            }
            OverrideSelector::Path(pattern) => path_glob_matches(pattern, path),
        }
    }

    fn specificity(&self) -> u32 {
        match &self.selector {
            OverrideSelector::Crate(_) => 1_000_000,
            OverrideSelector::Path(pattern) => path_glob_specificity(pattern),
        }
    }

    fn apply_severity(&self, current: &mut SeverityConfig) {
        if let Some(partial) = &self.severity {
            partial.findings.apply(&mut current.findings);
            partial.seams.apply(&mut current.seams);
        }
    }

    fn apply_oracles(&self, current: &mut OraclePolicy) {
        if let Some(partial) = &self.oracles {
            partial.apply(current);
        }
    }
}

fn path_glob_specificity(pattern: &str) -> u32 {
    let literal_segments = pattern
        .split('/')
        .filter(|segment| !segment.is_empty() && !segment.contains('*') && !segment.contains('?'))
        .count() as u32;
    let wildcard_penalty = u32::from(pattern.contains('*') || pattern.contains('?'));
    literal_segments
        .saturating_mul(10)
        .saturating_add(5u32.saturating_sub(wildcard_penalty * 5))
        .saturating_add((pattern.len() as u32).min(9))
}

pub(crate) fn normalize_repo_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn crate_name_for_path<'a>(crates: &'a [WorkspaceCrate], path: &str) -> Option<&'a str> {
    crates
        .iter()
        .filter(|member| path_under_package(path, &member.dir))
        .max_by_key(|member| member.dir.len())
        .map(|member| member.name.as_str())
}

fn path_under_package(path: &str, dir: &str) -> bool {
    if dir.is_empty() || dir == "." {
        return true;
    }
    path == dir || path.starts_with(&format!("{dir}/"))
}

impl RiprConfig {
    /// Bind Cargo package names/directories so `crate = "..."` selectors
    /// resolve. Unknown crate names fail closed.
    pub(crate) fn bind_workspace_crates(
        &mut self,
        crates: Vec<WorkspaceCrate>,
    ) -> Result<(), String> {
        reject_unknown_override_crates(&self.overrides, &crates)?;
        self.workspace_crates = crates;
        sync_compiled_overlays(self);
        Ok(())
    }

    pub(crate) fn severity_for_file(&self, file: &Path) -> SeverityConfig {
        let path = normalize_repo_path(file);
        self.severity_for(crate_name_for_path(&self.workspace_crates, &path), &path)
    }

    pub(crate) fn severity_for(&self, crate_name: Option<&str>, path: &str) -> SeverityConfig {
        let mut severity = self.severity.clone();
        severity.overlays.clear();
        if let Some(overlay) = best_override(&self.overrides, crate_name, path) {
            overlay.apply_severity(&mut severity);
        }
        severity
    }

    pub(crate) fn oracles_for(&self, crate_name: Option<&str>, path: &str) -> OraclePolicy {
        let mut oracles = self.oracles.clone();
        oracles.overlays.clear();
        if let Some(overlay) = best_override(&self.overrides, crate_name, path) {
            overlay.apply_oracles(&mut oracles);
        }
        oracles
    }
}

pub(crate) fn oracle_overlay_identity(overrides: &[ConfigOverride]) -> String {
    let mut encoded = overrides
        .iter()
        .filter(|row| {
            row.oracles
                .as_ref()
                .is_some_and(|oracles| !oracles.is_empty())
        })
        .map(|row| {
            let policy = row
                .oracles
                .as_ref()
                .map(PartialOraclePolicy::identity_value)
                .unwrap_or_default();
            format!("{}:{policy}", row.selector.identity_key())
        })
        .collect::<Vec<_>>();
    encoded.sort();
    format!("{}\u{0}{}", encoded.len(), encoded.join("\u{0}"))
}

impl SeverityConfig {
    /// Repo-wide severity, then the most specific matching `[[overrides]]`
    /// row. Overlays that only name a crate match after bind (resolved glob)
    /// or when the path sits under that package.
    pub(crate) fn for_exposure_in(&self, class: &ExposureClass, file: &Path) -> ConfigSeverity {
        self.resolved_for_path(file).for_exposure(class)
    }

    pub(crate) fn for_seam_in(&self, class: SeamGripClass, file: &Path) -> ConfigSeverity {
        self.resolved_for_path(file).for_seam(class)
    }

    pub(crate) fn resolved_for_path(&self, file: &Path) -> SeverityConfig {
        if self.overlays.is_empty() {
            return self.clone();
        }
        let path = normalize_repo_path(file);
        let mut resolved = self.clone();
        resolved.overlays.clear();
        if let Some(overlay) = best_override(&self.overlays, None, &path) {
            overlay.apply_severity(&mut resolved);
        }
        resolved
    }
}

impl OraclePolicy {
    pub(crate) fn has_overlays(&self) -> bool {
        !self.overlays.is_empty()
    }

    pub(crate) fn resolved_for_path(&self, file: &Path) -> OraclePolicy {
        if self.overlays.is_empty() {
            return self.clone();
        }
        let path = normalize_repo_path(file);
        let mut resolved = self.clone();
        resolved.overlays.clear();
        if let Some(overlay) = best_override(&self.overlays, None, &path) {
            overlay.apply_oracles(&mut resolved);
        }
        resolved
    }
}

fn best_override<'a>(
    overrides: &'a [ConfigOverride],
    crate_name: Option<&str>,
    path: &str,
) -> Option<&'a ConfigOverride> {
    overrides
        .iter()
        .enumerate()
        .filter(|(_, row)| row.matches(crate_name, path))
        .max_by_key(|(index, row)| (row.specificity(), *index))
        .map(|(_, row)| row)
}

pub(crate) fn sync_compiled_overlays(config: &mut RiprConfig) {
    refresh_resolved_globs(&mut config.overrides, &config.workspace_crates);
    config.severity.overlays = config
        .overrides
        .iter()
        .filter(|row| {
            row.severity
                .as_ref()
                .is_some_and(|severity| !severity.is_empty())
        })
        .cloned()
        .collect();
    config.oracles.overlays = config
        .overrides
        .iter()
        .filter(|row| {
            row.oracles
                .as_ref()
                .is_some_and(|oracles| !oracles.is_empty())
        })
        .cloned()
        .collect();
}

fn refresh_resolved_globs(overrides: &mut [ConfigOverride], crates: &[WorkspaceCrate]) {
    for row in overrides {
        if let OverrideSelector::Crate(name) = &row.selector {
            row.resolved_path_glob =
                crates
                    .iter()
                    .find(|member| member.name == *name)
                    .map(|member| {
                        if member.dir.is_empty() || member.dir == "." {
                            "**".to_string()
                        } else {
                            format!("{}/**", member.dir.trim_end_matches('/'))
                        }
                    });
        }
    }
}

fn reject_unknown_override_crates(
    overrides: &[ConfigOverride],
    crates: &[WorkspaceCrate],
) -> Result<(), String> {
    let known = crates
        .iter()
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();
    for row in overrides {
        let OverrideSelector::Crate(name) = &row.selector else {
            continue;
        };
        if known.iter().any(|known_name| *known_name == name) {
            continue;
        }
        return Err(unknown_crate_message(name, &known));
    }
    Ok(())
}

fn unknown_crate_message(name: &str, known: &[&str]) -> String {
    if known.is_empty() {
        format!(
            "[[overrides]] crate = \"{name}\" does not match a Cargo package in this workspace (no Cargo package names were found under the selected root)"
        )
    } else {
        format!(
            "[[overrides]] crate = \"{name}\" does not match a Cargo package in this workspace (known: {})",
            known.join(", ")
        )
    }
}

/// Package names and directories from `Cargo.toml` workspace members.
/// Malformed or missing manifests yield an empty list; crate-name overrides
/// then fail closed in [`RiprConfig::bind_workspace_crates`].
pub(crate) fn discover_workspace_crates(root: &Path) -> Vec<WorkspaceCrate> {
    let manifest = root.join("Cargo.toml");
    let Ok(text) = fs::read_to_string(&manifest) else {
        return Vec::new();
    };
    let Ok(document) = text.parse::<toml::Value>() else {
        return Vec::new();
    };
    let mut crates = Vec::new();
    if let Some(name) = package_name(&document) {
        crates.push(WorkspaceCrate {
            name,
            dir: String::new(),
        });
    }
    let Some(members) = document
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
    else {
        return crates;
    };
    for member in members {
        let Some(pattern) = member.as_str() else {
            continue;
        };
        for dir in expand_workspace_member(root, pattern) {
            let member_manifest = root.join(&dir).join("Cargo.toml");
            let Ok(member_text) = fs::read_to_string(&member_manifest) else {
                continue;
            };
            let Ok(member_document) = member_text.parse::<toml::Value>() else {
                continue;
            };
            if let Some(name) = package_name(&member_document) {
                crates.push(WorkspaceCrate { name, dir });
            }
        }
    }
    crates
}

fn package_name(document: &toml::Value) -> Option<String> {
    document
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
}

fn expand_workspace_member(root: &Path, pattern: &str) -> Vec<String> {
    let trimmed = pattern.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains("..") {
        return Vec::new();
    }
    if let Some(parent) = trimmed.strip_suffix("/*") {
        if parent.contains('*') || parent.contains('?') {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(root.join(parent)) else {
            return Vec::new();
        };
        let mut dirs = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !entry.path().join("Cargo.toml").is_file() {
                continue;
            }
            if parent.is_empty() {
                dirs.push(name.to_string());
            } else {
                dirs.push(format!("{parent}/{name}"));
            }
        }
        return dirs;
    }
    if trimmed.contains('*') || trimmed.contains('?') {
        return Vec::new();
    }
    vec![trimmed.to_string()]
}

pub(crate) fn parse_crate_selector(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("[[overrides]] crate must not be empty".to_string());
    }
    if trimmed.contains('\0') {
        return Err("[[overrides]] crate must not contain NUL characters".to_string());
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(format!(
            "[[overrides]] crate `{trimmed}` is not a Cargo package name; use letters, digits, `_`, or `-`"
        ));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn parse_path_selector(pattern: &str) -> Result<String, String> {
    let path = super::parse_relative_path("[[overrides]] path", pattern)?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}
