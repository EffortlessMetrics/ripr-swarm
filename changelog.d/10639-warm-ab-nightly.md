<!-- section: Added -->
- The nightly size-scaling workflow gains a `warm-ab` job: `cargo xtask warm-ab` times a warm `ripr check` of two pinned diffs of this repository with a baseline and a candidate build on one runner and fails when the candidate costs more than 1.5x the baseline. #10489 doubled that check and no gate failed (#10639).
