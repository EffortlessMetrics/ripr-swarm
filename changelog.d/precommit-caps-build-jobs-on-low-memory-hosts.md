<!-- section: Internal / CI -->
- Local `cargo xtask precommit` runs its Clippy and `covered_by` Cargo
  children with `CARGO_BUILD_JOBS=1` on a host with less than 24 GiB of memory
  and more than one CPU, and says so. Whole-workspace Clippy at higher job
  counts ran out of memory in 15 GB cloud containers. An explicit
  `CARGO_BUILD_JOBS` wins, and hosted `ci-policy` is unchanged (#13705).
