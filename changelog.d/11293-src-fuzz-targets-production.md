<!-- section: Fixed -->
- A cargo-fuzz harness is recognised only at `<pkg>/fuzz/fuzz_targets/**`; an ordinary library module under `src/fuzz/fuzz_targets/` is analyzed as production code again instead of silently reporting no changed behavior. Alternate harness layouts are registered through test-harness configuration (#11293).
