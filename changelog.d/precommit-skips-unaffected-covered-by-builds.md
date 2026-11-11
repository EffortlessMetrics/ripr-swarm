<!-- section: Internal / CI -->
- Local `cargo xtask precommit` no longer builds test binaries to list
  `covered_by` tests when the change set touches no Rust source, manifest,
  workspace-wide Rust control file or `policy/` path, and its reports say the
  listing was not run. That build held about 13.8 GB and was killed in 15 GB
  cloud containers. Hosted `ci-policy` and `cargo xtask check-file-policy`
  still list every applicable selector (#13705).
