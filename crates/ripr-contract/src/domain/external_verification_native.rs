//! Native Kani observations. These types do not establish source applicability or trust.

use serde::{Deserialize, Serialize};

/// Typed observations validated against the pinned export profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeRun {
    /// The export's schema version. The importer accepts only `0.1.0`.
    pub schema_version: String,
    /// Whether every selected harness was exported.
    pub run_state: NativeRunState,
    /// The `target` value recorded in the export.
    pub target: String,
    /// How many harnesses the run selected.
    pub matched_count: u64,
    /// Selected harnesses missing from the export: `matched_count` minus the
    /// imported harnesses.
    pub missing_count: u64,
    /// The imported harnesses, one per crate and harness name.
    pub harnesses: Vec<NativeHarness>,
}

/// Whether a native run exported every selected harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeRunState {
    /// At least one harness was selected and every selected harness was
    /// exported.
    Complete,
    /// Fewer harnesses were exported than were selected.
    Partial,
    /// No filters were given and no harnesses were selected.
    NoHarnessesSelected,
}

/// Successful checks have counts, without a complete exported property inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeHarness {
    /// The crate that defines the harness.
    pub crate_name: String,
    /// The harness name.
    pub name: String,
    /// The source file of the harness, as exported.
    pub file: String,
    /// The source line of the harness.
    pub line: u64,
    /// Whether the harness expects a panic (`#[kani::should_panic]`).
    pub should_panic: bool,
    /// How the harness run ended.
    pub outcome: NativeOutcome,
    /// Number of checks. Present exactly when the outcome is
    /// [`NativeOutcome::Completed`]; `None` for every other outcome.
    pub checks_total: Option<u64>,
    /// Number of successful checks. Present exactly when the outcome is
    /// [`NativeOutcome::Completed`]; `None` for every other outcome.
    pub checks_success: Option<u64>,
    /// Number of cover properties. Present exactly when the outcome is
    /// [`NativeOutcome::Completed`]; `None` for every other outcome.
    pub covers_total: Option<u64>,
}

/// How one harness run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeOutcome {
    /// Verification finished with a verdict.
    Completed {
        /// Whether the harness passed.
        verdict: NativeVerdict,
        /// Kani's failure classification for the run.
        failure_kind: NativeFailureKind,
    },
    /// Verification hit the harness timeout.
    Timeout,
    /// Verification ran out of memory.
    OutOfMemory,
    /// Verification crashed.
    Crashed {
        /// The exit code, when the export has one.
        code: Option<i32>,
        /// The crash message, when the export has one.
        message: Option<String>,
    },
}

/// The verdict of a completed harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeVerdict {
    /// The harness passed. For a `should_panic` harness this means it failed
    /// only with assertion failures.
    Success,
    /// The harness failed.
    Failure,
}

/// Kani's classification of a completed harness's failed properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeFailureKind {
    /// No property failed.
    None,
    /// Only assertion properties failed, which is what a `should_panic` harness
    /// expects.
    PanicsOnly,
    /// Some other property failed.
    Other,
    /// Kani reported an error. The export may show no errored property for it.
    Error,
}
