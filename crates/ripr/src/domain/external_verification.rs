//! Imported native observations remain separate from static exposure and receipts.

use serde::{Deserialize, Serialize};

pub(crate) const KANI_BINDING_SCHEMA: &str = "ripr_kani_binding.v1";
pub(crate) const KANI_REPORT_SCHEMA: &str = "ripr_kani_report.v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceFileRole {
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
pub(crate) struct KaniSourceFile {
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) roles: Vec<SourceFileRole>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KaniInvocationContext {
    pub(crate) target: String,
    pub(crate) cargo_features: Vec<String>,
    pub(crate) default_features: bool,
    pub(crate) toolchain: String,
    pub(crate) environment_sha256: String,
    /// Descriptive tokens only; no importer executes them.
    pub(crate) invocation: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KaniSelectedHarness {
    pub(crate) crate_name: String,
    pub(crate) name: String,
}
