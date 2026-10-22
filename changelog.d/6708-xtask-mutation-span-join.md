<!-- section: Fixed -->
- `cargo xtask mutation-calibration` keeps cargo-mutants runtime spans and
  location-conflict flags from the shared importer, leaves conflicting
  records unmatched, and joins by span containment so it agrees with
  `ripr calibrate cargo-mutants` (#6708).
