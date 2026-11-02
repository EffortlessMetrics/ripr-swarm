<!-- section: Changed -->
- `Pilot Measure` now builds `ripr` into a runner-temp target directory outside the pilot root, takes a `timeout_ms` dispatch input (default 1800000, clamped to 2700000 so the job cap leaves room for the timeout packet) for `ripr pilot --timeout-ms`, echoes its summary to the job log, and reports whether `repo-exposure.json` was written and its size (#11951).
