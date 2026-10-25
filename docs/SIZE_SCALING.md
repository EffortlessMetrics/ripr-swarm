# Size scaling gate

`cargo xtask size-scaling --binary <path>` runs `ripr check --diff` against one
exact `ripr` binary on generated files at one size, twice that, and four times
that, and fails when the cost of a file grows faster than its size (#10362). A
file that costs a minute is a hostile-input failure even when its output is
correct, and a quadratic term is invisible at the sizes real repositories use.
The TypeScript identifier-write and Python per-test copy fixed in #10330 and
#10359 were both found this way and both passed every functional check.

```bash
cargo build --release -p ripr
cargo xtask size-scaling --binary target/release/ripr
```

Outputs: `target/ripr/reports/size-scaling.{json,md}`. The workspaces are owned
and disposable under `target/ripr/size-scaling/<pid>`; a run refuses to reuse an
existing root and fails if it cannot remove its own.

## Matrix

Rust, Python and TypeScript, each in production and test roles, each as one
long line and as one item per line: twelve cells. In each cell the large file
sits beside a small changed file, and the diff touches only the small one, so
the large file is read as an unchanged neighbour, as it would be in a real
repository. The test cells carry many tests per file, which is the shape that
hid both fixed terms. `--languages rust,python` narrows the run.

## Decision

For each cell the judge takes the wall time at the three sizes and fails the
cell when:

- any step reaches the three-minute deadline, ends on a signal, or exits
  with a code other than 0 or 1;
- the largest step takes longer than `--ceiling-secs` (default 60);
- the cost of the largest step over the smallest, measured from at least 250 ms,
  exceeds `--max-doubling-ratio` (default 2.45) raised to the number of
  doublings. Linear is 2.0, so four times the size may cost up to six times as
  much.

A cell whose largest step finishes in under one second carries no growth
signal and passes. The thresholds are `Limits` in `size_scaling/judge.rs`, and
the judge is tested against synthetic linear, quadratic, over-ceiling and
incomplete series, and against a stub binary whose sleep grows with the square
of the input.

## What it does not claim

- Wall time only. Peak memory is not measured, so a term that is quadratic in
  memory but not in time passes (the Python case did both; memory needs its own
  instrument).
- One run per size. A noisy host can move one ratio; the allowance is slack
  and the cells at the edge of it are worth a rerun before a bug is filed.
- Debug and release builds give very different absolute numbers through the
  same ratio check. Only the ceiling is build-dependent; qualify with the
  release binary.
- Perl is not covered: `ripr` needs an external producer for it (#10364).
- It does not change product behavior, publish, or qualify a release.
