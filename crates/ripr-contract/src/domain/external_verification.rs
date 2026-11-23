//! Imported native observations remain separate from static exposure and receipts.

use serde::{Deserialize, Serialize};

/// Schema version a Kani binding file must declare to be imported.
pub const KANI_BINDING_SCHEMA: &str = "ripr_kani_binding.v1";
/// Schema version of the report that `ripr` writes for an imported Kani run.
pub const KANI_REPORT_SCHEMA: &str = "ripr_kani_report.v1";

/// What a source file listed in a Kani binding is for. A file may have several
/// roles, but never the same role twice. The importer requires at least one
/// `Implementation` file and one `Harness` file, and requires the file of every
/// exported harness to have the `Harness` role. The other roles are descriptive
/// labels it does not check further.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFileRole {
    /// Build or tool configuration.
    Configuration,
    /// Source of a dependency.
    Dependency,
    /// Source that defines Kani harnesses.
    Harness,
    /// Source of the code under verification.
    Implementation,
    /// A lockfile.
    Lockfile,
    /// A specification the harnesses check against.
    Specification,
    /// Toolchain configuration.
    Toolchain,
}

/// One source file a Kani binding lists, with its expected digest. The importer
/// reads each listed file under the checked root and rejects the import if its
/// bytes do not hash to `sha256`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KaniSourceFile {
    /// Path relative to the checked root. It must use `/` separators and have
    /// no empty, `.` or `..` parts, backslashes or colons.
    pub path: String,
    /// Expected content digest, as `sha256:` followed by 64 lowercase hex
    /// characters.
    pub sha256: String,
    /// What the file is for. Must be nonempty with no repeated role.
    pub roles: Vec<SourceFileRole>,
}

/// The recorded context of the Kani invocation. The importer checks these
/// fields' form and that `target` matches the native export; it does not re-run
/// or recompute anything from them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KaniInvocationContext {
    /// Target name. Must equal the target the native export reports.
    pub target: String,
    /// Cargo features enabled for the run. Entries must be unique.
    pub cargo_features: Vec<String>,
    /// Whether Cargo's default features were enabled.
    pub default_features: bool,
    /// Toolchain description, as recorded.
    pub toolchain: String,
    /// Digest of the recorded environment, as `sha256:` followed by 64
    /// lowercase hex characters. The importer checks only its form.
    pub environment_sha256: String,
    /// Descriptive tokens only; no importer executes them.
    pub invocation: Vec<String>,
}

/// One harness the binding says was selected. The selected set must match the
/// native export's matched harness count, and every exported harness must be in
/// it.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KaniSelectedHarness {
    /// The crate that defines the harness.
    pub crate_name: String,
    /// The harness name.
    pub name: String,
}
