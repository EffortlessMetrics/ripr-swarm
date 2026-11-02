<!-- section: Fixed -->
- Inline `assert_eq!(f(..).unwrap_err(), E::V)` (and matching `expect_err` / `matches!` spellings) now classify as `ExactErrorVariant` so ErrorVariant seams can credit direct variant pins without a binding (#12266).
