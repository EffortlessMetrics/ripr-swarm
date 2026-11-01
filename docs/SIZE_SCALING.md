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

A nightly advisory lane (`.github/workflows/size-scaling.yml`) builds the release
binary and runs the gate at a 512 KiB base, so the largest step is 2 MiB; the
report is kept as the `size-scaling` artifact, which is the trend. It is not
part of the required PR workflow.

Outputs: `target/ripr/reports/size-scaling.{json,md}`. The workspaces are owned
and disposable under `target/ripr/size-scaling/<pid>`; a run refuses to reuse an
existing root and fails if it cannot remove its own.

## Matrix

Rust, Python and TypeScript, each in production and test roles, each as one
long line and as one item per line: eleven cells. A Python test is a `def`
and cannot share a line with the next, so there is no one-line Python test cell. In each cell the large file
sits beside a small changed file, and the diff touches only the small one, so
the large file is read as an unchanged neighbour, as it would be in a real
repository. The test cells carry many tests per file, which is the shape that
hid both fixed terms. `--languages rust,python` narrows the run.

Every step is run from a private copy of the binary, and its sha256 is recorded
in the report as `binary_sha256`, so a rebuild during the run cannot change what
was measured.

## Decision

For each cell the judge takes the wall time at the three sizes and fails the
cell when:

- a step exits 0 or 1 but its JSON report has no finding for the small changed
  file, or a limitation names the large file (`not_analyzed`). A fast exit that
  never read the file is not a pass;
- any step reaches the three-minute deadline, ends on a signal, or exits
  with a code other than 0 or 1;
- the largest step takes longer than `--ceiling-secs` (default 60);
- the cost of the largest step over the smallest, measured from at least 250 ms,
  exceeds `--max-doubling-ratio` (default 2.45) raised to the number of
  doublings. Linear is 2.0, so four times the size may cost up to six times as
  much. The middle step has no verdict of its own: the endpoint ratio over 4x
  the size is less noisy than a single doubling.

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

## Warm-check A/B on ripr's own diffs (#10639)

The size cells are far smaller than a real large test file, and the
dx-scoreboard speed row goes uncompared when the baseline runner class differs
from the current one, which is the normal case. #10489 doubled the warm check of
this repository's own two-xtask-file diff (about 31 s to 64 s) and merged with
every required check green. `cargo xtask warm-ab` is the row that fails.

```bash
cargo xtask warm-ab --baseline <bin> --candidate <bin> --tree <checkout> --diff <file> --pin <id> [--control-slowdown <ratio>]
```

It copies both binaries, gives each its own `RIPR_CACHE_DIR`, runs one cold
check each to fill the caches, then `--runs` (default 3) warm checks alternating
baseline and candidate, so a drifting runner touches both alike. Runner class
and load cancel because the comparison is inside one job. It fails when:

- the candidate's median warm time is more than `--max-ratio` (default 1.5) times
  the baseline's and the difference is over a 2 s noise floor;
- the candidate's median exceeds `--ceiling-secs` (default 300);
- a run times out (10 minutes), ends on a signal, exits with a code other than
  0 or 1, or prints a report that is not JSON or has no `findings` array;
- either binary reports no finding in any measured check (cold or warm) for
  the pinned diff, so that side analyzed nothing. A one-sided empty report
  fails even when the other side still finds something.

The report (`warm-ab-<pin>.{json,md}`) records both binaries' sha256, cold and
warm milliseconds, finding counts, and whether the two findings reports are
byte-identical. A difference is reported, not failed: the two are different
builds.

`--control-slowdown <ratio>` (above 1.0, at most 10.0) is the negative control
(#11876, #12244): after each candidate warm check the harness waits `(ratio - 1)`
times that run's paired baseline time, so the candidate reads `ratio` times the
baseline on every pin, whether it takes two seconds or a minute. A fixed sleep
crossed the limit only on the slow-baseline pins (5 s on a 1.9 s warm check is
4x; on a 57 s one it is 1.09x). The packet records `control_slowdown`, so a
control receipt cannot be mistaken for a measurement, and no product code is
touched. Never pass it in the nightly measurement.

The nightly `warm-ab` job in `size-scaling.yml` runs
`cargo xtask warm-ab --baseline-ref <ref> --candidate-ref <ref> --pins <id=commit,...> --external-pins <corpus ids>`,
which builds the candidate release binary at the run's commit and the baseline at
the newest first-parent commit at least 24 hours older (`baseline_ref` and
`candidate_ref` override both for `workflow_dispatch`), makes a worktree and diff
for each pin, and runs the A/B on two pins, each a fixed commit of this repository whose tree and diff do not
drift with main, and on the external pins (#11876): `--external-pins rust-analyzer,tokio,servo` names entries of the
scoreboard corpus in `benchmarks/dx_scoreboard/scoreboards.toml`, each cloned at its pinned commit and timed on
the diff the scoreboard row measures (`base_sha` when set, else the commit's parent). An id the corpus does not
declare is an error, and a pin id may not be used by both lists:

| Pin | Commit | Shape |
|---|---|---|
| `xtask-tests` | `a58975de` | two `xtask` files including the 57,000-line `xtask/src/tests.rs`: tests times file size |
| `product-rust` | `af8136f7` | product Rust under `crates/ripr` |

The negative control is the pre-#10638 build as candidate against the post-#10638
build as baseline: it must fail the `xtask-tests` pin. Run locally on 2026-10-26 (release builds of `a58975de` and `3fa4ad28`, 4 cores, 3 warm runs each, interleaved): the pre-fix build as candidate failed at 2.17x (medians 24.4 s baseline, 52.9 s candidate), and with the roles swapped the fix passed at 0.46x. The judge also has a unit
test on the #10489 shape (about 31 s against about 64 s).

Not claimed: wall time only; a regression under the ratio or the noise floor
passes; two pinned diffs of this repository and one pinned change for each external repository do not cover other shapes of change. The external pins have no recorded negative control yet; it is the same pre-fix `candidate_ref` dispatch, run after the step first lands. The default
baseline is the newest first-parent commit at least 24 hours older than the
candidate, so the detection window is a single night: if one nightly run is
missed, skipped or never acted on, the next baseline can be the regression
itself and the gate compares it against itself. Scheduled and dispatched
runs use separate concurrency groups so a manual dispatch cannot cancel the
nightly mid-measurement, but a missed night still consumes the window.
