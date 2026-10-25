<!-- section: Internal / CI -->
- `cargo xtask repo-exposure-latency-report` now compares cache-served output
  with a cold recomputation for each format (a private `RIPR_CACHE_DIR`: cold
  run, then a run that must show a cache hit). `output_parity` is `pass`,
  `mismatch` or `not_checked`, so `status: pass` is reachable (report schema
  0.5); a mismatch is `fail`, and `release-readiness` treats it as a required
  failure while other non-pass reports stay a non-required `warn` (#9633).
- Classified-seam and corpus-fingerprint cache store failures now have bounded
  `{path, stage, error}` rows (`classified_seam_store`,
  `corpus_fingerprint_store`, at most 32 plus `store_failures_dropped`) in the
  opt-in latency trace and the latency report, and block `pass` (#9633).
- A cache read through a path component that is a file is a cache miss instead
  of also being counted as `corrupt_ignored`; the failed write remains the one
  reported store failure (#9633).
- The latency report no longer trusts any stderr line that starts with a
  receipt prefix: receipts must carry a per-run token passed in
  `RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN`; forged lines are ignored (#9633).
- Parity legs are compared only when the producer's own subject receipt (cache
  key and generated-disclosure digests) agrees across the cold and served runs;
  movement is `not_checked` (`comparison_subject_moved`), not a mismatch. A
  store failure on a parity leg now names a `parity_*` status reason and keeps
  the report from `pass` (#9633).
