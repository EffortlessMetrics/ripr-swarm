<!-- section: Changed -->
- Rust checks run the macro-generated-test, proptest, local-empty-macro and
  withheld-routing per-file parses in parallel (order preserved), reuse the
  cached index within a run for an identical file set, and report the diff
  admission and main-index stages through `RIPR_REPO_EXPOSURE_LATENCY_TRACE`
  (#12947, #12985).
