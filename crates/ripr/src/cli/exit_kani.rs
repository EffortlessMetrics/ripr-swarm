//! Kani proofs for the process exit-code contract (`docs/EXIT_CODES.md`).
//!
//! Run: `cargo kani -p ripr --harness <name>`.

use super::exit_proof_domain::{COMMAND_ERROR_CASES, cover_command_errors};
use super::{EXIT_COMPLETED, EXIT_COULD_NOT_COMPLETE, EXIT_DECISION_OR_REFUSAL};

/// The documented table: 0 completed, 2 could not complete, 3 declined.
#[kani::proof]
fn exit_code_constants_match_documented_table() {
    assert_eq!(EXIT_COMPLETED, 0);
    assert_eq!(EXIT_COULD_NOT_COMPLETE, 2);
    assert_eq!(EXIT_DECISION_OR_REFUSAL, 3);
    kani::cover!(
        EXIT_COMPLETED == 0 && EXIT_COULD_NOT_COMPLETE == 2 && EXIT_DECISION_OR_REFUSAL == 3,
        "the documented exit-code checks are reached"
    );
}

/// `CommandError::exit_code` is total over the closed variant set, lands only
/// on the documented non-zero codes (an error never exits 0), and maps each
/// variant to its documented code. Messages are fixed to empty strings; this
/// does not establish message handling or CLI dispatch. The constructor list,
/// exhaustive compiler check and input covers come from one declaration.
#[kani::proof]
fn command_error_exit_code_total_and_matches_table() {
    let which: usize = kani::any();
    // Every constructor in the compiler-checked CommandError domain.
    kani::assume(which < COMMAND_ERROR_CASES.len());
    let (error, expected) = &COMMAND_ERROR_CASES[which];
    cover_command_errors(error);
    let code = error.exit_code();
    assert_eq!(code, *expected);
    assert!(code != EXIT_COMPLETED);
}
