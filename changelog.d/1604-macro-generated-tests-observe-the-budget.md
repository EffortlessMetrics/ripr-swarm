<!-- section: Fixed -->
- `ripr review-comments` now stops at its `--timeout-ms` bound while resolving macro-generated tests: that pass re-parsed files with no cancellation checkpoint, so on a large workspace a 30-second bound ran about 45 seconds in some runs (observed on a debug build) before the timeout receipt was written (#1604).
