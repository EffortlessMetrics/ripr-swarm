<!-- section: Fixed -->
- Test harness: `cli_smoke` now installs copies of the built `ripr` by hard link instead of byte copy, so no writable descriptor can leak into a forked sibling and make the exec fail with `Text file busy` (#11449).
