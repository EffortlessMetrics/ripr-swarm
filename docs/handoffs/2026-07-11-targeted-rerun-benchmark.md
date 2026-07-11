# Targeted rerun benchmark receipt

Recorded for `RIPR-SPEC-0123` on commit `5a266472c26e230d86e1b5982e7191b7b3c652b5`.

Command:

```text
cargo xtask targeted-rerun-benchmark \
  --root benchmarks/targeted_rerun_benchmark/input \
  --changed-test tests/targeted.rs \
  --samples 3 \
  --timeout-ms 120000
```

Runner: `local-windows-x86_64`  
Analyzer: `ripr 0.10.0`  
Cache: isolated `RIPR_CACHE_DIR`, reset between cold and invalidation samples

Measured receipt (`target/ripr/reports/targeted-rerun-benchmark.json`):

| Measure | Result |
| --- | ---: |
| Cold full p50 | 1,309 ms |
| Cold full p95 | 1,518 ms |
| Warm targeted p50 | 104 ms |
| Warm targeted p95 | 205 ms |
| Cold-to-warm p50 speedup | 12.5865x |
| Warm p50 target | 30,000 ms |
| Parity | `matched` |
| Explicit invalidation | `recomputed_file_facts` after cache reset |
| Receipt status | `pass` |

This is a named benchmark for the committed fixture, revision, configuration,
and runner class. It does not claim universal latency, runtime mutation
behavior, correctness, coverage adequacy, or complete broader-input
invalidation attribution. The targeted-rerun receipt continues to report
broader invalidation as `not_available` until the analyzer can attribute those
inputs from owned facts.
