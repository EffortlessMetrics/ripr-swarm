<!-- section: Changed -->
- `Pilot Measure` now builds `ripr` into a runner-temp target directory outside the pilot root, takes a `timeout_ms` dispatch input (default 1800000) for `ripr pilot --timeout-ms`, echoes its summary to the job log, and reports whether `repo-exposure.json` was written and its size (#11951).
