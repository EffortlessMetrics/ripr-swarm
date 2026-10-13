<!-- section: Fixed -->
- `ripr calibrate cargo-mutants --help` now says how to install cargo-mutants,
  run `cargo mutants`, and pass `mutants.out` (or `outcomes.json` /
  `mutants.json`). The targeted-test workflow recipe produces that directory
  instead of assuming a `target/ripr/workflow/cargo-mutants.json` file already
  exists (#7450).
