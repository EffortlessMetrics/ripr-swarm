<!-- section: Fixed -->
- `ripr check` no longer treats a cargo-fuzz harness (`fuzz/fuzz_targets/*.rs`, a declared `[[bin]]` of the nested fuzz package) as a production edit target: it is test-like harness surface, so a diff touching only it no longer headlines a test gap against an unrelated test (#11293).
