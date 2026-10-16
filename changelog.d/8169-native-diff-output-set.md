<!-- section: Added -->
- `ripr check --diff FILE --output-dir NEW_DIRECTORY` writes native diff badge,
  detail JSON and human reports from one analysis, with input drift checks and
  a final output-set manifest (#8169).
- Selected roots spelled through directory symlinks retain native output parity;
  input-read failures are scoped to their active root boundary (#8169).
- Reads through aliases under excluded Git metadata or Cargo target directories
  remain refused even when the alias resolves to captured source (#8169).
- Refusal sets publish to the captured canonical output destination once custody
  exists, so a destination parent retargeted mid-run never receives them (#8169).
