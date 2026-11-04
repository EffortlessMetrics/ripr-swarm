<!-- section: Fixed -->
- The flake ledger now covers the 14 open intermittent-test issues recorded in a checked-in issue snapshot, and `xtask/tests/flake_ledger.rs` fails when an open flaky issue has no ledger row or a row disagrees with its issue state; wall-clock-bound rows carry `timing_dependent = true` (#12936).
