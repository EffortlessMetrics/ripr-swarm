//! Imported native observations remain separate from static exposure and receipts.

use serde::{Deserialize, Serialize};

pub const KANI_BINDING_SCHEMA: &str = "ripr_kani_binding.v1";
pub const KANI_REPORT_SCHEMA: &str = "ripr_kani_report.v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFileRole {
    Configuration,
    Dependency,
    Harness,
    Implementation,
    Lockfile,
    Specification,
    Toolchain,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KaniSourceFile {
    pub path: String,
    pub sha256: String,
    pub roles: Vec<SourceFileRole>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KaniInvocationContext {
    pub target: String,
    pub cargo_features: Vec<String>,
    pub default_features: bool,
    pub toolchain: String,
    pub environment_sha256: String,
    /// Descriptive tokens only; no importer executes them.
    pub invocation: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KaniSelectedHarness {
    pub crate_name: String,
    pub name: String,
}
