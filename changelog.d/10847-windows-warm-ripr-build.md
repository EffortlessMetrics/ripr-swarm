<!-- section: Fixed -->
- The sharded Windows advisory lane warms `cargo build -p ripr` before nextest,
  so the tests that spawn a nested `cargo build` (the public-guide contract, the
  fixture and dogfood runners, the policy cargo spawns) no longer queue behind
  one cold per-process compile and hit the 360 s kill (#10847).
