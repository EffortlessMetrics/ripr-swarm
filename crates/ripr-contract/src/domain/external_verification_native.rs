//! Native Kani observations. These types do not establish source applicability or trust.

use serde::{Deserialize, Serialize};

/// Typed observations validated against the pinned export profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeRun {
    pub schema_version: String,
    pub run_state: NativeRunState,
    pub target: String,
    pub matched_count: u64,
    pub missing_count: u64,
    pub harnesses: Vec<NativeHarness>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeRunState {
    Complete,
    Partial,
    NoHarnessesSelected,
}

/// Successful checks have counts, without a complete exported property inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeHarness {
    pub crate_name: String,
    pub name: String,
    pub file: String,
    pub line: u64,
    pub should_panic: bool,
    pub outcome: NativeOutcome,
    pub checks_total: Option<u64>,
    pub checks_success: Option<u64>,
    pub covers_total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeOutcome {
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
pub enum NativeVerdict {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeFailureKind {
    None,
    PanicsOnly,
    Other,
    Error,
}
