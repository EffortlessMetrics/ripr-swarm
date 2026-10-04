<!-- section: Added -->
- `cargo xtask pilot-ranking`: a checked-in answer key for `ripr pilot`'s top
  picks. Five small crates are pinned to exact commits with the cargo-mutants
  outcome of each of their 1,647 viable mutants, so `score` reports top-5 and
  top-10 precision, scored share and distinct functions in seconds without
  rerunning cargo-mutants. The new `ranking` scoreboard gates the pooled
  numbers nightly, on demand, and when a pull request that touches pilot
  ranking or seam grading leaves draft.
