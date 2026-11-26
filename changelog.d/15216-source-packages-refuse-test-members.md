<!-- section: Fixed -->
- The `ripr` and `ripr-contract` source packages no longer ship test-only
  modules, Kani proof harnesses or test fixture data, and `cargo xtask
  publish-dry-run` refuses any packaged member that only a test or Kani build
  compiles (#15216).
