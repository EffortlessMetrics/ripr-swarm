<!-- section: Added -->
- `ripr check` now has a wall-clock budget: `--timeout SECS` or
  `RIPR_CHECK_TIMEOUT=<seconds>`, default 600, `0` disables it. When the
  budget runs out the run stops and refuses with `analysis_timeout`
  (route `analysis/timeout`) in every machine format that has a refusal
  document, instead of running with no bound. The refusal
  carries no findings and is never a clean pass (#5369).
