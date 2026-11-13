//! Pinned native tool identity and process-result laws for shared interpretation.
//! These pure checks inspect supplied observations and grant no execution authority.

use serde::{Deserialize, Serialize};

const KANI_VERSION: &str = "0.68.0";
const CBMC_VERSION: &str = "6.11.0";

/// An explicit provider configuration request, never an observation that the
/// named compiler, solver, target, or flags actually ran.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredKaniProfile {
    pub kani_version: String,
    pub cbmc_version: String,
    pub target: String,
    pub configuration_policy: String,
    pub default_unwind: u64,
    pub lib: bool,
    pub default_features: bool,
    pub features: Vec<String>,
    pub all_features: bool,
}

impl DeclaredKaniProfile {
    pub fn pinned() -> Self {
        Self {
            kani_version: KANI_VERSION.to_string(),
            cbmc_version: CBMC_VERSION.to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            configuration_policy: "kani-0.68-defaults-v1".to_string(),
            default_unwind: 32,
            lib: true,
            default_features: true,
            features: Vec::new(),
            all_features: false,
        }
    }

    /// A distinct reviewed request profile. This does not claim default-feature
    /// equivalence, Cargo metadata admission, or execution of any native claim.
    pub fn rust_only() -> Self {
        Self {
            configuration_policy: "kani-0.68-rust-only-v1".to_string(),
            default_features: false,
            features: vec!["lang-rust".to_string()],
            ..Self::pinned()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self != &Self::pinned() && self != &Self::rust_only() {
            return Err(
                "declared native profile differs from a pinned ordinary Kani profile".to_string(),
            );
        }
        Ok(())
    }
}

/// Aggregate observed verification processes without borrowing individual
/// harness verdicts. The live producer and retained replay use this same law.
pub fn aggregate_process(
    invocations: &[super::Invocation],
    raw_index_digest: &str,
) -> crate::formal_evidence::Process {
    use crate::formal_evidence::{Cleanup, Process, ProcessState};
    let verifications: Vec<_> = invocations
        .iter()
        .filter(|call| call.phase == "verification")
        .collect();
    let started = !verifications.is_empty();
    let timed_out = verifications.iter().any(|call| call.timed_out);
    let all_exited = started
        && verifications
            .iter()
            .all(|call| call.capture_complete && call.exit_code.is_some() && call.error.is_none());
    let exit_code = all_exited.then(|| {
        verifications
            .iter()
            .filter_map(|call| call.exit_code)
            .find(|code| *code != 0)
            .unwrap_or(0)
    });
    Process {
        state: if timed_out {
            ProcessState::TimedOut
        } else if !started {
            ProcessState::NotStarted
        } else if exit_code == Some(0) {
            ProcessState::Completed
        } else {
            ProcessState::Failed
        },
        exit_code,
        timed_out,
        raw_output_digest: started.then(|| raw_index_digest.to_string()),
        cleanup_state: if !started {
            Cleanup::NotApplicable
        } else if invocations.iter().all(|call| call.capture_complete) {
            Cleanup::Complete
        } else {
            Cleanup::NotObserved
        },
    }
}

pub fn native_tool_header(stdout: &str) -> Result<(), String> {
    let kani: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("Kani Rust Verifier "))
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    let cbmc: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("CBMC "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|version| *version != "version")
        .collect();
    if kani != [KANI_VERSION]
        || cbmc.is_empty()
        || cbmc.iter().any(|version| *version != CBMC_VERSION)
    {
        return Err("native tool headers do not match pinned Kani0.68.0/CBMC6.11.0".to_string());
    }
    Ok(())
}
