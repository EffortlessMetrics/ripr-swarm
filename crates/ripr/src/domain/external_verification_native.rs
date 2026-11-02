//! Native Kani observations. These types do not establish source applicability or trust.

use serde::{Deserialize, Serialize};

/// Typed observations validated against the pinned export profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativeRun {
    pub(crate) schema_version: String,
    pub(crate) run_state: NativeRunState,
    pub(crate) target: String,
    pub(crate) matched_count: u64,
    pub(crate) missing_count: u64,
    pub(crate) harnesses: Vec<NativeHarness>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum NativeRunState {
    Complete,
    Partial,
    NoHarnessesSelected,
}

/// Successful checks have counts, without a complete exported property inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeHarness {
    pub(crate) crate_name: String,
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: u64,
    pub(crate) should_panic: bool,
    pub(crate) outcome: NativeOutcome,
    pub(crate) checks_total: Option<u64>,
    pub(crate) checks_success: Option<u64>,
    pub(crate) covers_total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum NativeOutcome {
    Completed {
        verdict: NativeVerdict,
        failure_kind: NativeFailureKind,
    },
    Timeout,
    OutOfMemory,
    Crashed {
        code: Option<i32>,
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum NativeVerdict {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum NativeFailureKind {
    None,
    PanicsOnly,
    Other,
    Error,
}
