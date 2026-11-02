//! Shared project, package, source-role, and test-environment facts (#5541 /
//! #6364). Pure DTOs and digest law only; ecosystem producers project into
//! these shapes without changing command behavior.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub(crate) const PROJECT_FACTS_SCHEMA_VERSION: &str = "project_facts.v1";

/// Canonical cross-language source role (#6364). Language-native roles may
/// appear in `SourceUnitFactV1::native_role` when mapping would lose detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CanonicalSourceRole {
    Production,
    Test,
    TestHelper,
    Fixture,
    Generated,
    Vendor,
    Example,
    Config,
    Resource,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FactCurrentnessV1 {
    /// Which analysis-input keys must stay stable for this row to remain current.
    pub input_keys: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceUnitFactV1 {
    pub path: String,
    pub package_id: String,
    pub role: CanonicalSourceRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_role: Option<String>,
    pub basis: String,
    pub currentness: FactCurrentnessV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TestEnvironmentFactV1 {
    pub environment_id: String,
    pub framework: String,
    pub runner: String,
    pub package_root: String,
    /// Working directory a typed verify route must use (distinct from repository root).
    pub required_cwd: String,
    pub runtime_class: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub config_input_keys: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProjectFactsV1 {
    pub schema_version: String,
    pub language: String,
    /// Portable repository identity: normalized relative layout, not host path.
    pub repository_identity: String,
    pub repository_root_cwd: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_ids: Vec<String>,
    pub source_units: Vec<SourceUnitFactV1>,
    pub test_environments: Vec<TestEnvironmentFactV1>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub config_input_keys: BTreeSet<String>,
    pub currentness: FactCurrentnessV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
    pub semantic_digest: String,
}

impl ProjectFactsV1 {
    pub(crate) fn seal(mut facts: ProjectFactsV1) -> Result<ProjectFactsV1, String> {
        facts.schema_version = PROJECT_FACTS_SCHEMA_VERSION.to_string();
        facts.package_ids.sort();
        facts.package_ids.dedup();
        facts
            .source_units
            .sort_by(|left, right| left.path.cmp(&right.path));
        facts
            .test_environments
            .sort_by(|left, right| left.environment_id.cmp(&right.environment_id));
        facts.semantic_digest = semantic_digest(&facts)?;
        Ok(facts)
    }
}

pub(crate) fn semantic_digest(facts: &ProjectFactsV1) -> Result<String, String> {
    #[derive(Serialize)]
    struct DigestInput<'a> {
        schema_version: &'a str,
        language: &'a str,
        repository_identity: &'a str,
        package_ids: &'a [String],
        source_units: &'a [SourceUnitFactV1],
        test_environments: &'a [TestEnvironmentFactV1],
        config_input_keys: &'a BTreeSet<String>,
        currentness: &'a FactCurrentnessV1,
        limitations: &'a [String],
    }
    let input = DigestInput {
        schema_version: &facts.schema_version,
        language: &facts.language,
        repository_identity: &facts.repository_identity,
        package_ids: &facts.package_ids,
        source_units: &facts.source_units,
        test_environments: &facts.test_environments,
        config_input_keys: &facts.config_input_keys,
        currentness: &facts.currentness,
        limitations: &facts.limitations,
    };
    let json = serde_json::to_vec(&input)
        .map_err(|error| format!("project facts digest serialization failed: {error}"))?;
    Ok(crate::config::bytes_fingerprint(&json))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_unit(path: &str, role: CanonicalSourceRole) -> SourceUnitFactV1 {
        SourceUnitFactV1 {
            path: path.to_string(),
            package_id: "pkg".to_string(),
            role,
            native_role: None,
            basis: "layout".to_string(),
            currentness: FactCurrentnessV1 {
                input_keys: BTreeSet::from(["workspace_manifests".to_string()]),
            },
            limitations: Vec::new(),
        }
    }

    #[test]
    fn semantic_digest_ignores_host_specific_repository_root_cwd_when_identity_matches()
    -> Result<(), String> {
        let mut left = ProjectFactsV1 {
            schema_version: PROJECT_FACTS_SCHEMA_VERSION.to_string(),
            language: "rust".to_string(),
            repository_identity: "layout:single-package".to_string(),
            repository_root_cwd: ".".to_string(),
            package_ids: vec!["pkg".to_string()],
            source_units: vec![sample_unit("src/lib.rs", CanonicalSourceRole::Production)],
            test_environments: Vec::new(),
            config_input_keys: BTreeSet::new(),
            currentness: FactCurrentnessV1 {
                input_keys: BTreeSet::from(["workspace_manifests".to_string()]),
            },
            limitations: Vec::new(),
            semantic_digest: String::new(),
        };
        left = ProjectFactsV1::seal(left)?;
        let mut right = left.clone();
        right.repository_root_cwd = "nested/checkout/root".to_string();
        right.semantic_digest.clear();
        right = ProjectFactsV1::seal(right)?;
        assert_eq!(left.semantic_digest, right.semantic_digest);
        Ok(())
    }

    #[test]
    fn semantic_digest_changes_when_source_role_changes() -> Result<(), String> {
        let sealed = ProjectFactsV1::seal(ProjectFactsV1 {
            schema_version: PROJECT_FACTS_SCHEMA_VERSION.to_string(),
            language: "rust".to_string(),
            repository_identity: "layout:single-package".to_string(),
            repository_root_cwd: ".".to_string(),
            package_ids: vec!["pkg".to_string()],
            source_units: vec![sample_unit("src/lib.rs", CanonicalSourceRole::Production)],
            test_environments: Vec::new(),
            config_input_keys: BTreeSet::new(),
            currentness: FactCurrentnessV1 {
                input_keys: BTreeSet::from(["workspace_manifests".to_string()]),
            },
            limitations: Vec::new(),
            semantic_digest: String::new(),
        })?;
        let mut wrong = sealed.clone();
        wrong.source_units[0].role = CanonicalSourceRole::Test;
        wrong.semantic_digest.clear();
        wrong = ProjectFactsV1::seal(wrong)?;
        assert_ne!(sealed.semantic_digest, wrong.semantic_digest);
        Ok(())
    }
}
