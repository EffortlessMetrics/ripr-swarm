<!-- section: Internal / CI -->
- `policy/flaky-tests.toml` records the LSP disk-equality read-count flake fixed
  by #13912, with a guard test that fails if the test reset clears the shared
  digest cache again (#13839).
