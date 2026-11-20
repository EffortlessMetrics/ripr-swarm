<!-- section: Fixed -->
- Test-only: the seam-inventory tests that compare workspace cache keys or mappings now hold the seam-limit mutator lock, so a concurrent pilot or repair test can no longer flip the key mid-test and fail the Rust-only feature lane (#13393).
