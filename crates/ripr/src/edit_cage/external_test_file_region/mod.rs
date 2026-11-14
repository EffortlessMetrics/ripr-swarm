//! Pure-insertion content cage for an external Rust test file (#8710).

mod attempt;

#[cfg(test)]
#[path = "attempt_tests.rs"]
mod attempt_tests;

pub(crate) use attempt::{
    ExternalTargetCopies, ExternalTestFileBaseline, ExternalTestFileObservation,
    capture_attempt_external_region, observe_attempt_external_region, uncommitted_target_refusal,
};
