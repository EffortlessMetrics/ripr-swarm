<!-- section: Fixed -->
- `ripr gate evaluate` now says how to proceed when `baseline-check` or
  `calibrated-gate` has no usable baseline (start with `--mode visible-only`,
  then `ripr baseline create`), and when `--gap-ledger` is given a document
  that is not a gap decision ledger such as a repo-exposure report (build one
  with `ripr reports gap-ledger --check-output`, or use `--pr-guidance`)
  (#11206).
