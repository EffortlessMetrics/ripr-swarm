//! Producer-owned Integration `NewTestTargetProposal` admission (#4576).
//!
//! When no existing test can own a test-only repair, this module may earn
//! one exact new integration-test file from RustIndex facts. It does not
//! invent expected values or generate the test body. Inline unit insertion
//! stays out of scope.

#[cfg(test)]
mod tests;
