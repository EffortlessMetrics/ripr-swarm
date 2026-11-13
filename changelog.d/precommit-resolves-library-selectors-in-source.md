<!-- section: Internal / CI -->
- Local `cargo xtask precommit` no longer builds the `ripr` library test
  target to list one `covered_by` selector. A library unit-test selector is
  checked against source (the module is declared and holds a test function)
  and its report row says it was not listed. That build held about 13.8 GB
  and ran out of memory in 15 GB cloud containers. Hosted `ci-policy` and
  `cargo xtask check-file-policy` still build and list every selector (#13705).
