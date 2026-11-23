//! Shared project, package, source-role, and test-environment facts (#5541 /
//! #6364). Pure DTOs and digest law only; ecosystem producers project into
//! these shapes without changing command behavior.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Schema version stamped on every [`ProjectFactsV1`].
pub const PROJECT_FACTS_SCHEMA_VERSION: &str = "project_facts.v1";

/// Canonical cross-language source role (#6364). Language-native roles may
/// appear in `SourceUnitFactV1::native_role` when mapping would lose detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalSourceRole {
    /// Production source.
    Production,
    /// A test.
    Test,
    /// Code that supports tests without being a test itself.
    TestHelper,
    /// A fixture.
    Fixture,
    /// Generated source.
    Generated,
    /// Vendored third-party source.
    Vendor,
    /// An example.
    Example,
    /// Configuration.
    Config,
    /// A non-code resource.
    Resource,
    /// The role could not be determined.
    Unknown,
}

/// The analysis inputs a fact depends on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactCurrentnessV1 {
    /// Which analysis-input keys must stay stable for this row to remain current.
    pub input_keys: BTreeSet<String>,
}

/// The role of one source file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceUnitFactV1 {
    /// Repository-relative path, with `/` separators.
    pub path: String,
    /// The package the file belongs to.
    pub package_id: String,
    /// The canonical cross-language role.
    pub role: CanonicalSourceRole,
    /// The language-native role, when the canonical role loses detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_role: Option<String>,
    /// Short code naming the rule that assigned the role, such as
    /// `rust_source_role:production_subject`.
    pub basis: String,
    /// The inputs this row depends on.
    pub currentness: FactCurrentnessV1,
    /// Limitations that apply to this row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

/// One way to run a package's tests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestEnvironmentFactV1 {
    /// Stable id for the environment, such as `cargo-test:<package dir>`.
    pub environment_id: String,
    /// The test framework, such as `libtest`.
    pub framework: String,
    /// The command that runs the tests, such as `cargo`.
    pub runner: String,
    /// Repository-relative package directory.
    pub package_root: String,
    /// Working directory a typed verify route must use (distinct from repository root).
    pub required_cwd: String,
    /// The toolchain class the tests need, such as `rust_toolchain`.
    pub runtime_class: String,
    /// Configuration inputs that can change how the tests run.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub config_input_keys: BTreeSet<String>,
    /// Limitations that apply to this environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

/// Project, package, source-role and test-environment facts for one repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFactsV1 {
    /// Always [`PROJECT_FACTS_SCHEMA_VERSION`] once sealed.
    pub schema_version: String,
    /// The language these facts describe, such as `rust`.
    pub language: String,
    /// Portable repository identity: normalized relative layout, not host path.
    pub repository_identity: String,
    /// Working directory for repository-wide commands, relative to the
    /// repository root.
    pub repository_root_cwd: String,
    /// The packages that own at least one source row, not every package in the
    /// repository.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_ids: Vec<String>,
    /// One row per source file.
    pub source_units: Vec<SourceUnitFactV1>,
    /// One row per test environment.
    pub test_environments: Vec<TestEnvironmentFactV1>,
    /// Configuration inputs that apply across the repository.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub config_input_keys: BTreeSet<String>,
    /// The inputs the whole record depends on.
    pub currentness: FactCurrentnessV1,
    /// Limitations that apply to the whole record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
    /// Digest over every field except this one and `repository_root_cwd`.
    /// Package ids, source rows and environment rows are sorted first;
    /// `limitations` keep their order, so reordering them changes the digest.
    pub semantic_digest: String,
}
