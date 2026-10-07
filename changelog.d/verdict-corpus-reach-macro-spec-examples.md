<!-- section: Added -->
- The Rust verdict corpus gains 15 authored cases, each in its own crate, for
  numbered acceptance examples of the reach, macro and miss-evidence specs
  that no case cited: RIPR-SPEC-0114 examples 2, 4 and 6; 0115 examples 1 to 3;
  0117 examples 1 to 4; 0118 examples 2 to 4; 0119 examples 1 and 2; 0120
  examples 1 and 2; 0125 example 1; and 0224 example 1. The labels come from 41
  mutants run against each crate's own `cargo test` and replayed with
  `verdict-corpus relabel`. 0114 example 3 (a crate with no tests) cannot be
  labeled, because relabel needs at least one test to run. On this base ripr
  reads 3 cases ideally, abstains on 8 and gives 4 false actionable verdicts
  (#6974, #6975, #6614).
