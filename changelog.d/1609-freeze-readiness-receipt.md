<!-- section: Internal / CI -->
- `cargo xtask freeze-readiness --candidate <full-sha>` evaluates the eight
  swarm freeze predicates against one exact commit and writes
  `target/ripr/reports/freeze-readiness.{json,md}`. It reads Git objects at the
  named commit, never the working tree or an earlier receipt, compares the
  #2766 packet with Git ancestry and the landings its rows cite, accepts a
  hosted `cargo-deny` result only with proof of which commit it scanned, and
  exits nonzero unless all eight pass (#1609).
