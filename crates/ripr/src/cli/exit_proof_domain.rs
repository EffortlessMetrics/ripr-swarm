//! Closed constructor domain for the command-error exit proof and ordinary tests.
//!
//! One declaration generates the input cases, a wildcard-free compiler check,
//! and per-constructor Kani covers. An added variant or an omitted case fails
//! compilation; duplicate cases fail the unreachable-pattern check. The literal
//! exit-code oracle comes from `docs/EXIT_CODES.md`, not `exit_code()` or its
//! production constants. Messages are fixed to empty strings in this domain.

use super::CommandError;

macro_rules! command_error_domain {
    ($($variant:ident => $expected:literal),+ $(,)?) => {
        pub(super) const COMMAND_ERROR_CASES: &[(CommandError, i32)] = &[
            $((CommandError::$variant(String::new()), $expected),)+
        ];

        const _: fn(&CommandError) = |error| {
            #[deny(unreachable_patterns)]
            match error {
                $(CommandError::$variant(_) => (),)+
            }
        };

        #[cfg(kani)]
        pub(super) fn cover_command_errors(error: &CommandError) {
            $(kani::cover!(matches!(error, CommandError::$variant(_)));)+
        }
    };
}

command_error_domain!(
    Failure => 2,
    Decision => 3,
    Interrupted => 130,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_error_domain_is_nonempty_and_unique() {
        fn check(cases: &[(CommandError, i32)]) {
            assert!(!cases.is_empty());
            for (index, (error, _)) in cases.iter().enumerate() {
                assert!(
                    !cases[..index].iter().any(|(prior, _)| prior == error),
                    "duplicate constructor: {error:?}"
                );
                assert!(error.message().is_empty());
            }
        }
        check(COMMAND_ERROR_CASES);
    }

    #[test]
    fn every_command_error_matches_the_documented_exit_code() {
        for (error, expected) in COMMAND_ERROR_CASES {
            let code = error.exit_code();
            assert_eq!(code, *expected, "{error:?}");
            assert_ne!(code, 0, "an error cannot report completion: {error:?}");
        }
    }
}
