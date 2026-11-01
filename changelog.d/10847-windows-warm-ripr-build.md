<!-- section: Fixed -->
- The sharded Windows advisory lane builds `ripr` once before nextest, so tests
  that spawn a nested `cargo build` (the public-guide contract, the fixture and
  dogfood runners, the policy cargo spawns) find the build already done instead
  of each compiling it cold and queueing on the target lock until the 360 s
  kill. In a verification run none of the ten recurred; one different test
  timed out and is tracked in #12035 (#10847).
