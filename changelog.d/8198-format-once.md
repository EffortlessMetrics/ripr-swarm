<!-- section: Internal / CI -->
- `cargo xtask ci-policy` leaves formatting to the required hosted preflight,
  avoiding a duplicate formatter invocation while local precommit retains it (#8198).
