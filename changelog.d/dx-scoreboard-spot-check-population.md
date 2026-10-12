<!-- section: Changed -->
- `cargo xtask dx-scoreboard` compares a mutation spot-check rate only against
  a baseline measured over the same repositories, revisions, cargo-mutants
  runs and mutant timeout, and names both populations when it refuses. A run
  with an unrecorded cargo-mutants version is never compared (#6311).
