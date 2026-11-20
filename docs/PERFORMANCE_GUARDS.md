# Performance regression guards

Which check notices when `ripr` gets slower to run, larger in what it prints,
or slower to start, and where that check runs. Each guard is a regression
ceiling over a fixed subject. None of them is a product guarantee, and passing
one says nothing about workloads it does not measure (#12906).

| Axis | Guard | Subject | Signal | Runs |
| --- | --- | --- | --- | --- |
| Startup | `cli_smoke::startup_budget` | `--version`, `--help`, `check --help`, `capabilities` | the process's own CPU time and peak resident memory (#5213 receipt); best-of-five wall time against a loose ceiling; identical output from an unparsable working directory | with the `ripr` test suite |
| Output size | `cli_smoke::agent_output_budget` | one five-gap Rust diff through the outputs an agent reads | bytes and lines per output (#13673) | with the `ripr` test suite |
| Run time, classifier work | `cli_smoke::check_work_budget` | the same five-gap Rust diff, one worker | indexed reads the classifier reports per probe (`RIPR_PROBE_READ_TRACE`), summed (#15049) | with the `ripr` test suite |
| Run time, growth | [Size scaling gate](SIZE_SCALING.md) | generated Rust, Python and TypeScript files at 1x, 2x, 4x | cost ratio per doubling | nightly `size-scaling.yml` |
| Run time, this repository | [warm-ab](SIZE_SCALING.md#warm-check-ab-on-riprs-own-diffs-10639) | pinned diffs, baseline build against candidate build on one runner | candidate over baseline ratio | nightly `size-scaling.yml` |
| Run time, consumers | [dx-scoreboard](METRICS.md) | pinned external repositories | warm-check ceilings | nightly `dx-scoreboard.yml` |

## Startup budget

The four startup commands do no analysis, so what they cost is what every
invocation pays before it starts work. The test runs each five times from this
repository's root with `RIPR_REPO_EXPOSURE_LATENCY_TRACE` set and reads the
`ripr_resource_cost_receipt` line. It fails when:

- the largest peak resident memory is over the command's ceiling (48 MiB for
  `--version` and `--help`, 64 MiB for `check --help` and `capabilities`);
- CPU time summed over the five runs is over 100 ms;
- the fastest of the five runs takes longer than two seconds;
- a receipt is missing, repeated, reports a number as unavailable, or reports a
  peak under 1 MiB. A measurement that was not taken does not pass as zero.

CPU time and peak memory do not rise when the runner is busy, which is why they
carry the signal and wall time only catches a gross regression. Measured on
2026-11-11 (Linux x86_64, dev profile, 100 runs per command): peaks of 23.0 to
25.3 MiB and 31.4 to 36.4 MiB, CPU time under one 10 ms clock tick in every
run, wall time 5 to 14 ms. With 40 MiB of touched memory and a CPU loop added
ahead of command dispatch the test failed on `--version` at 64.0 MiB and
11,060 ms; the judge and the receipt reader have their own refusal tests.

A second test starts the same commands in a directory whose `ripr.toml`,
`Cargo.toml` and source do not parse and requires the same output as from an
empty directory, so a workspace read added ahead of dispatch that acts on what
it read fails on every platform with no timing involved. A read whose result
and errors are both discarded leaves the output unchanged and passes this test.

Limits:

- The memory and CPU ceilings are judged on Linux only. The receipt is also
  observed on Windows, but no Windows measurement backs these ceilings.
- A run with `LLVM_PROFILE_FILE` set (coverage instrumentation) prints that it
  was not judged and passes.
- The ceilings have about 1.75x headroom, so a few megabytes of added startup
  memory pass. CPU time under one clock tick per run is invisible on a kernel
  that samples it.
- Dev-profile numbers. A release binary starts smaller; the ceilings are upper
  bounds for both.

A change that needs more startup cost raises the ceiling in the same change and
says why. `cargo test -p ripr --test cli_smoke -- startup_ --nocapture` prints
one `startup-budget` line per command.

## Classifier work budget

Wall time cannot be judged per change without flaking, so this guard counts
work. The test checks the five-gap diff once with `RIPR_PROBE_READ_TRACE` set
and `RAYON_NUM_THREADS=1`, reads the `ripr_probe_read_trace` row of each
classifier call, and sums `observed_read_events`, the number of indexed reads
that call made. It fails when:

- there are not exactly 8 rows, or a row reports an interrupted or unreturned
  classification;
- the sum is over 1,150 reads;
- the sum is under 500 reads, which would mean the trace stopped counting or
  the subject shrank;
- a row does not parse or lacks a field.

Measured on 2026-11-18 (Linux x86_64, dev profile): 996 reads in 44 of 44
one-worker runs, with a cold and with a warm cache. With four workers the sum
was 1,002 to 1,011 over 50 runs, because workers race to fill the shared run
memo; that is why the test pins one worker. With eight redundant index reads
per related test added to the classifier the test failed at 1,316 reads; the
judge and the row reader have their own refusal tests.

The ceiling has about 15% headroom on purpose. Analyzer changes add a few reads
for good reasons, and a pin that every such change must bump would stop meaning
anything. The regressions this guard is for (reading the index again per probe,
per test or per oracle) multiply the count.

Limits:

- It counts indexed reads by the Rust classifier. Time spent inside one read,
  index construction, parsing, rendering and every other language are not
  counted, so a quadratic scan inside a single read passes.
- One small diff. A cost that only appears with many files or many related
  tests is left to the nightly lanes.
- A row's `dropped_read_events` is not judged: it counts reads missing from the
  row's per-site detail (64 distinct reads per row), not reads missing from the
  total.

`cargo test -p ripr --test cli_smoke -- check_work_budget --nocapture` prints
the `check-work-budget` line.
