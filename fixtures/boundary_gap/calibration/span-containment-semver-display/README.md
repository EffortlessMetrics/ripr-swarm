# Span Containment Calibration: semver `display.rs:20`

Negative regression for the `span_containment` join (#5486), from the #5336
report. In `dtolnay/semver` at `280ebcb6edac3aa4cdc545dbff8a26c5ac4861fe`,
`src/display.rs` line 20 holds a `call_presence` seam around
`digits(self.minor)` (columns 19 to 37) and a separate `+` operator at column
17. cargo-mutants 27.1.0 replaces that `+`.

The line-only join paired both `+` mutants with the call seam. With spans the
call seam does not contain the mutated range, so both mutants are unmatched
with `no_containing_seam` and no seam is scored against them.

The inputs are reduced metadata, not source: the one seam's identity, kind,
location, span and grip class, and the two mutants' name, file, span,
replacement, genre and outcome.

## Update Command

```bash
ripr calibrate cargo-mutants --mutants-json fixtures/boundary_gap/calibration/span-containment-semver-display/runtime-mutants.json --repo-exposure-json fixtures/boundary_gap/calibration/span-containment-semver-display/repo-exposure.json --format json > fixtures/boundary_gap/calibration/span-containment-semver-display/mutation-calibration.json
```

Use `--format md` for `mutation-calibration.md`.
