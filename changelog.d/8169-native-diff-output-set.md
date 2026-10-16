<!-- section: Added -->
- `ripr check --diff FILE --output-dir NEW_DIRECTORY` writes native diff badge,
  detail JSON and human reports from one analysis, with input drift checks and
  a final output-set manifest (#8169).
- Selected roots spelled through directory symlinks retain native output parity;
  input-read failures are scoped to their active root boundary (#8169).
