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
