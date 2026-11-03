//! Kani proofs for the process exit-code contract (`docs/EXIT_CODES.md`).
//!
//! Run: `cargo kani -p ripr --harness <name>`.

use super::{CommandError, EXIT_COMPLETED, EXIT_COULD_NOT_COMPLETE, EXIT_DECISION_OR_REFUSAL};

/// The documented table: 0 completed, 2 could not complete, 3 declined.
#[kani::proof]
fn exit_code_constants_match_documented_table() {
    assert_eq!(EXIT_COMPLETED, 0);
    assert_eq!(EXIT_COULD_NOT_COMPLETE, 2);
    assert_eq!(EXIT_DECISION_OR_REFUSAL, 3);
}

/// `CommandError::exit_code` is total over the closed variant set, lands only
/// on the documented non-zero codes (an error never exits 0), and maps each
/// variant to its documented code. Message content is irrelevant (empty).
#[kani::proof]
fn command_error_exit_code_total_and_matches_table() {
    let which: u8 = kani::any();
    kani::assume(which < 3); // exactly the three CommandError variants
    let (error, expected) = match which {
        0 => (CommandError::Failure(String::new()), 2),
        1 => (CommandError::Decision(String::new()), 3),
        _ => (CommandError::Interrupted(String::new()), 130),
    };
    let code = error.exit_code();
    kani::cover!(code == 2);
    kani::cover!(code == 3);
    kani::cover!(code == 130);
    assert_eq!(code, expected);
    assert!(code != EXIT_COMPLETED);
}
