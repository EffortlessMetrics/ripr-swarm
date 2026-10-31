<!-- section: Fixed -->
- `ripr reports ci-packet` no longer logs a declined `ripr agent start` (exit 3)
  as a failed step: it logs that the agent loop was skipped because ripr
  declined the seam, while other nonzero exits still fail with their exit code
  (#11206).
