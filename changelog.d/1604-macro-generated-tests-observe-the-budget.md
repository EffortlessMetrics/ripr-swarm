<!-- section: Fixed -->
- `ripr review-comments` now stops at its `--timeout-ms` bound while resolving macro-generated tests: that pass re-parsed every file with no cancellation checkpoint, so on a large workspace a 30-second bound could run about 45 seconds before the timeout receipt was written (#1604).
