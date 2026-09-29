//! Checked distribution identity for the PyPI and npm adapters (#4488).
//!
//! The workspace Cargo version remains the only product-version source. This
//! module maps that version into registry representations and binds the one
//! native target set to package identities before any adapter can be built.

use std::collections::BTreeSet;
use std::fs;

use serde::Deserialize;

use crate::write_report;

mod reports;
mod version;
mod workflow;

use reports::{distribution_json, distribution_markdown};
use version::{map_pep440_version, parse_workspace_version};
use workflow::{parse_workflow_targets, validate_workflow_projection};

pub(super) const DISTRIBUTION_MANIFEST_PATH: &str = "policy/distribution-packages.toml";
pub(super) const WORKSPACE_MANIFEST_PATH: &str = "Cargo.toml";
const RIPR_MANIFEST_PATH: &str = "crates/ripr/Cargo.toml";
pub(super) const ARCHIVE_WORKFLOW_PATH: &str = ".github/workflows/server-archive-qualification.yml";

const RULE_SCHEMA: &str = "distribution_schema";
const RULE_IDENTITY: &str = "distribution_identity";
pub(super) const RULE_VERSION: &str = "distribution_version";
const RULE_CRATE: &str = "distribution_crate";
const RULE_TARGET: &str = "distribution_target";
pub(super) const RULE_WORKFLOW: &str = "distribution_workflow";

const EXPECTED_SCHEMA_VERSION: &str = "0.1";
const EXPECTED_CONTROL_ISSUE: u32 = 4488;
const EXPECTED_COMPATIBILITY_ISSUE: u32 = 4489;
const EXPECTED_PRODUCT: &str = "ripr";
const EXPECTED_PYPI_DISTRIBUTION: &str = "ripr-rs";
const EXPECTED_NPM_LAUNCHER: &str = "@effortlessmetrics/ripr";
const EXPECTED_SOURCE_VERSION: &str = "workspace";
const EXPECTED_RELEASE_FEATURES: &[&str] = &["lang-rust", "lang-typescript", "lang-python"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct DistributionManifest {
    pub(super) schema_version: String,
    pub(super) control_issue: u32,
    pub(super) compatibility_issue: u32,
    pub(super) source_version: String,
    pub(super) non_claim: String,
    pub(super) product: String,
    pub(super) cargo_package: String,
    pub(super) executable: String,
    pub(super) pypi_distribution: String,
    pub(super) npm_launcher: String,
    pub(super) release_features: Vec<String>,
    #[serde(rename = "target")]
    pub(super) targets: Vec<DistributionTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct DistributionTarget {
    pub(super) rust_target: String,
    pub(super) executable: String,
    pub(super) native_os: String,
    pub(super) native_cpu: String,
    pub(super) native_libc: String,
    pub(super) minimum_system: String,
    pub(super) wheel_family: String,
    pub(super) npm_os: String,
    pub(super) npm_cpu: String,
    pub(super) npm_libc: Option<String>,
    pub(super) npm_package: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkflowTarget {
    pub(super) rust_target: String,
    pub(super) executable: String,
}

#[derive(Debug, Default)]
pub(super) struct DistributionOutcome {
    pub(super) violations: Vec<String>,
    pub(super) cargo_version: Option<String>,
    pub(super) pypi_version: Option<String>,
    pub(super) npm_version: Option<String>,
    pub(super) manifest: Option<DistributionManifest>,
    pub(super) workflow_targets: Vec<WorkflowTarget>,
}

pub(crate) fn check_distribution_packages() -> Result<(), String> {
    let mut read_violations = Vec::new();
    let manifest = read_text(DISTRIBUTION_MANIFEST_PATH, &mut read_violations);
    let workspace = read_text(WORKSPACE_MANIFEST_PATH, &mut read_violations);
    let ripr = read_text(RIPR_MANIFEST_PATH, &mut read_violations);
    let workflow = read_text(ARCHIVE_WORKFLOW_PATH, &mut read_violations);

    let mut outcome = match (manifest, workspace, ripr, workflow) {
        (Some(manifest), Some(workspace), Some(ripr), Some(workflow)) => {
            evaluate_distribution_contract(&manifest, &workspace, &ripr, &workflow)
        }
        _ => DistributionOutcome::default(),
    };
    read_violations.append(&mut outcome.violations);
    outcome.violations = read_violations;

    write_report("distribution-packages.json", &distribution_json(&outcome)?)?;
    write_report("distribution-packages.md", &distribution_markdown(&outcome))?;

    if outcome.violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "distribution package contract failed with {} violation(s); see target/ripr/reports/distribution-packages.md",
            outcome.violations.len()
        ))
    }
}

fn read_text(path: &str, violations: &mut Vec<String>) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) => {
            violations.push(rule(
                RULE_SCHEMA,
                path,
                &format!("cannot read required distribution input: {error}"),
            ));
            None
        }
    }
}

fn evaluate_distribution_contract(
    manifest_text: &str,
    workspace_text: &str,
    ripr_text: &str,
    workflow_text: &str,
) -> DistributionOutcome {
    let mut outcome = DistributionOutcome::default();
    let manifest = parse_distribution_manifest(manifest_text, &mut outcome.violations);
    let cargo_version = parse_workspace_version(workspace_text, &mut outcome.violations);
    validate_ripr_manifest(ripr_text, &manifest, &mut outcome.violations);

    if let Some(version) = &cargo_version {
        match map_pep440_version(version) {
            Ok(pypi_version) => outcome.pypi_version = Some(pypi_version),
            Err(error) => {
                outcome
                    .violations
                    .push(rule(RULE_VERSION, WORKSPACE_MANIFEST_PATH, &error))
            }
        }
        outcome.npm_version = Some(version.clone());
    }
    outcome.cargo_version = cargo_version;

    if let Some(manifest) = &manifest {
        validate_distribution_manifest(manifest, &mut outcome.violations);
    }

    outcome.workflow_targets = parse_workflow_targets(workflow_text, &mut outcome.violations);
    if let Some(manifest) = &manifest {
        validate_workflow_projection(
            &manifest.targets,
            &outcome.workflow_targets,
            &mut outcome.violations,
        );
    }
    outcome.manifest = manifest;
    outcome
}

fn parse_distribution_manifest(
    text: &str,
    violations: &mut Vec<String>,
) -> Option<DistributionManifest> {
    match toml::from_str(text) {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            violations.push(rule(
                RULE_SCHEMA,
                DISTRIBUTION_MANIFEST_PATH,
                &format!("cannot parse typed distribution manifest: {error}"),
            ));
            None
        }
    }
}

fn validate_ripr_manifest(
    text: &str,
    manifest: &Option<DistributionManifest>,
    violations: &mut Vec<String>,
) {
    let value: toml::Value = match toml::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            violations.push(rule(
                RULE_CRATE,
                RIPR_MANIFEST_PATH,
                &format!("cannot parse ripr crate manifest: {error}"),
            ));
            return;
        }
    };

    let package_name = value
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str);
    if package_name != Some(EXPECTED_PRODUCT) {
        violations.push(rule(
            RULE_CRATE,
            RIPR_MANIFEST_PATH,
            &format!(
                "Cargo package must be `{EXPECTED_PRODUCT}`, got `{}`",
                package_name.unwrap_or("missing")
            ),
        ));
    }

    let version_inherits_workspace = value
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_table)
        .and_then(|version| version.get("workspace"))
        .and_then(toml::Value::as_bool)
        == Some(true);
    if !version_inherits_workspace {
        violations.push(rule(
            RULE_CRATE,
            RIPR_MANIFEST_PATH,
            "`[package] version.workspace = true` must retain the workspace version authority",
        ));
    }

    let binary_names = value
        .get("bin")
        .and_then(toml::Value::as_array)
        .map(|bins| {
            bins.iter()
                .filter_map(toml::Value::as_table)
                .filter_map(|bin| bin.get("name"))
                .filter_map(toml::Value::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if binary_names.len() != 1 || binary_names.first().copied() != Some(EXPECTED_PRODUCT) {
        violations.push(rule(
            RULE_CRATE,
            RIPR_MANIFEST_PATH,
            &format!(
                "the released crate must expose exactly one `ripr` binary, got {binary_names:?}"
            ),
        ));
    }

    let default_features = value
        .get("features")
        .and_then(|features| features.get("default"))
        .and_then(toml::Value::as_array)
        .map(|features| {
            features
                .iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let expected_features = expected_release_features();
    if default_features != expected_features {
        violations.push(rule(
            RULE_CRATE,
            RIPR_MANIFEST_PATH,
            &format!(
                "crate default features must equal the distribution feature set {expected_features:?}, got {default_features:?}"
            ),
        ));
    }
    if let Some(manifest) = manifest
        && manifest.release_features != default_features
    {
        violations.push(rule(
            RULE_CRATE,
            DISTRIBUTION_MANIFEST_PATH,
            &format!(
                "release_features must match the crate defaults {default_features:?}, got {:?}",
                manifest.release_features
            ),
        ));
    }
}

fn validate_distribution_manifest(manifest: &DistributionManifest, violations: &mut Vec<String>) {
    let fixed_values = [
        (
            "schema_version",
            manifest.schema_version.as_str(),
            EXPECTED_SCHEMA_VERSION,
            RULE_SCHEMA,
        ),
        (
            "source_version",
            manifest.source_version.as_str(),
            EXPECTED_SOURCE_VERSION,
            RULE_VERSION,
        ),
        (
            "product",
            manifest.product.as_str(),
            EXPECTED_PRODUCT,
            RULE_IDENTITY,
        ),
        (
            "cargo_package",
            manifest.cargo_package.as_str(),
            EXPECTED_PRODUCT,
            RULE_IDENTITY,
        ),
        (
            "executable",
            manifest.executable.as_str(),
            EXPECTED_PRODUCT,
            RULE_IDENTITY,
        ),
        (
            "pypi_distribution",
            manifest.pypi_distribution.as_str(),
            EXPECTED_PYPI_DISTRIBUTION,
            RULE_IDENTITY,
        ),
        (
            "npm_launcher",
            manifest.npm_launcher.as_str(),
            EXPECTED_NPM_LAUNCHER,
            RULE_IDENTITY,
        ),
    ];
    for (field, actual, expected, rule_id) in fixed_values {
        if actual != expected {
            violations.push(rule(
                rule_id,
                DISTRIBUTION_MANIFEST_PATH,
                &format!("`{field}` must be `{expected}`, got `{actual}`"),
            ));
        }
    }

    if manifest.control_issue != EXPECTED_CONTROL_ISSUE {
        violations.push(rule(
            RULE_SCHEMA,
            DISTRIBUTION_MANIFEST_PATH,
            &format!(
                "`control_issue` must remain #{EXPECTED_CONTROL_ISSUE}, got #{}",
                manifest.control_issue
            ),
        ));
    }
    if manifest.compatibility_issue != EXPECTED_COMPATIBILITY_ISSUE {
        violations.push(rule(
            RULE_SCHEMA,
            DISTRIBUTION_MANIFEST_PATH,
            &format!(
                "`compatibility_issue` must remain #{EXPECTED_COMPATIBILITY_ISSUE}, got #{}",
                manifest.compatibility_issue
            ),
        ));
    }
    if manifest.non_claim.trim().is_empty() {
        violations.push(rule(
            RULE_SCHEMA,
            DISTRIBUTION_MANIFEST_PATH,
            "`non_claim` must state that identity does not establish publication or compatibility",
        ));
    }

    let expected_features = expected_release_features();
    if manifest.release_features != expected_features {
        violations.push(rule(
            RULE_IDENTITY,
            DISTRIBUTION_MANIFEST_PATH,
            &format!(
                "release_features must be the canonical ordered set {expected_features:?}, got {:?}",
                manifest.release_features
            ),
        ));
    }

    validate_targets(&manifest.targets, violations);
}

fn expected_release_features() -> Vec<String> {
    EXPECTED_RELEASE_FEATURES
        .iter()
        .map(|feature| (*feature).to_string())
        .collect()
}

fn validate_targets(targets: &[DistributionTarget], violations: &mut Vec<String>) {
    let expected = expected_targets();
    if targets.len() != expected.len() {
        violations.push(rule(
            RULE_TARGET,
            DISTRIBUTION_MANIFEST_PATH,
            &format!(
                "target matrix must contain exactly {} rows, got {}",
                expected.len(),
                targets.len()
            ),
        ));
    }

    let mut rust_targets = BTreeSet::new();
    let mut npm_packages = BTreeSet::new();
    for target in targets {
        if !rust_targets.insert(target.rust_target.as_str()) {
            violations.push(rule(
                RULE_TARGET,
                DISTRIBUTION_MANIFEST_PATH,
                &format!("duplicate Rust target `{}`", target.rust_target),
            ));
        }
        if !npm_packages.insert(target.npm_package.as_str()) {
            violations.push(rule(
                RULE_TARGET,
                DISTRIBUTION_MANIFEST_PATH,
                &format!("duplicate npm payload package `{}`", target.npm_package),
            ));
        }
        if target.minimum_system.trim().is_empty() {
            violations.push(rule(
                RULE_TARGET,
                DISTRIBUTION_MANIFEST_PATH,
                &format!(
                    "target `{}` must name a compatibility floor or an explicit pending qualification",
                    target.rust_target
                ),
            ));
        }
    }

    for expected_target in expected {
        match targets
            .iter()
            .find(|target| target.rust_target == expected_target.rust_target)
        {
            Some(actual) if target_identity_matches(actual, &expected_target) => {}
            Some(actual) => violations.push(rule(
                RULE_TARGET,
                DISTRIBUTION_MANIFEST_PATH,
                &format!(
                    "target `{}` does not match its canonical identity: expected {expected_target:?}, got {actual:?}",
                    expected_target.rust_target
                ),
            )),
            None => violations.push(rule(
                RULE_TARGET,
                DISTRIBUTION_MANIFEST_PATH,
                &format!("missing required Rust target `{}`", expected_target.rust_target),
            )),
        }
    }
    for target in targets {
        if !expected
            .iter()
            .any(|expected_target| expected_target.rust_target == target.rust_target)
        {
            violations.push(rule(
                RULE_TARGET,
                DISTRIBUTION_MANIFEST_PATH,
                &format!("unexpected distribution target `{}`", target.rust_target),
            ));
        }
    }
}

fn target_identity_matches(actual: &DistributionTarget, expected: &DistributionTarget) -> bool {
    actual.rust_target == expected.rust_target
        && actual.executable == expected.executable
        && actual.native_os == expected.native_os
        && actual.native_cpu == expected.native_cpu
        && actual.native_libc == expected.native_libc
        && actual.wheel_family == expected.wheel_family
        && actual.npm_os == expected.npm_os
        && actual.npm_cpu == expected.npm_cpu
        && actual.npm_libc == expected.npm_libc
        && actual.npm_package == expected.npm_package
}

fn expected_targets() -> Vec<DistributionTarget> {
    vec![
        DistributionTarget {
            rust_target: "x86_64-pc-windows-msvc".to_string(),
            executable: "ripr.exe".to_string(),
            native_os: "windows".to_string(),
            native_cpu: "x86_64".to_string(),
            native_libc: "msvc".to_string(),
            minimum_system: "pending:#4489".to_string(),
            wheel_family: "windows".to_string(),
            npm_os: "win32".to_string(),
            npm_cpu: "x64".to_string(),
            npm_libc: None,
            npm_package: "@effortlessmetrics/ripr-win32-x64-msvc".to_string(),
        },
        DistributionTarget {
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            executable: "ripr".to_string(),
            native_os: "linux".to_string(),
            native_cpu: "x86_64".to_string(),
            native_libc: "glibc".to_string(),
            minimum_system: "pending:#4489".to_string(),
            wheel_family: "manylinux".to_string(),
            npm_os: "linux".to_string(),
            npm_cpu: "x64".to_string(),
            npm_libc: Some("glibc".to_string()),
            npm_package: "@effortlessmetrics/ripr-linux-x64-gnu".to_string(),
        },
        DistributionTarget {
            rust_target: "aarch64-unknown-linux-gnu".to_string(),
            executable: "ripr".to_string(),
            native_os: "linux".to_string(),
            native_cpu: "aarch64".to_string(),
            native_libc: "glibc".to_string(),
            minimum_system: "pending:#4489".to_string(),
            wheel_family: "manylinux".to_string(),
            npm_os: "linux".to_string(),
            npm_cpu: "arm64".to_string(),
            npm_libc: Some("glibc".to_string()),
            npm_package: "@effortlessmetrics/ripr-linux-arm64-gnu".to_string(),
        },
        DistributionTarget {
            rust_target: "x86_64-apple-darwin".to_string(),
            executable: "ripr".to_string(),
            native_os: "macos".to_string(),
            native_cpu: "x86_64".to_string(),
            native_libc: "system".to_string(),
            minimum_system: "pending:#4489".to_string(),
            wheel_family: "macosx".to_string(),
            npm_os: "darwin".to_string(),
            npm_cpu: "x64".to_string(),
            npm_libc: None,
            npm_package: "@effortlessmetrics/ripr-darwin-x64".to_string(),
        },
        DistributionTarget {
            rust_target: "aarch64-apple-darwin".to_string(),
            executable: "ripr".to_string(),
            native_os: "macos".to_string(),
            native_cpu: "aarch64".to_string(),
            native_libc: "system".to_string(),
            minimum_system: "pending:#4489".to_string(),
            wheel_family: "macosx".to_string(),
            npm_os: "darwin".to_string(),
            npm_cpu: "arm64".to_string(),
            npm_libc: None,
            npm_package: "@effortlessmetrics/ripr-darwin-arm64".to_string(),
        },
    ]
}

pub(super) fn rule(id: &str, path: &str, message: &str) -> String {
    format!("{id} :: {path} {message}")
}

#[cfg(test)]
mod tests;
