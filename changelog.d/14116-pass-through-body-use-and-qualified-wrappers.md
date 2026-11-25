<!-- section: Fixed -->
- A `proptest!` property whose body imports a trait (`use
  std::os::unix::ffi::OsStrExt;`) or calls `proptest::prop_oneof!` and the
  other `prop_*` macros through the `proptest::` path now confines its
  argument mentions to the call, as a property without them already did. A
  glob or renaming `use`, an import of a trusted macro name, and a nested
  call under any other path keep the workspace-wide refusal (RIPR-SPEC-0369,
  #14116).
