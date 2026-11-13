<!-- section: Internal / CI -->
- `cargo xtask dx-scoreboard` records the compiler that produced its owned
  Cargo binary and compares measured speed only across matching compiler
  identities. External binaries and historical baselines retain unknown
  provenance; completion loss and absolute ceilings still fail the gate (#14539).
