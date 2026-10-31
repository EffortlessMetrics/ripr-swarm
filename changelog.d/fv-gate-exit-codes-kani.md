<!-- section: Internal / CI -->
- Add Kani bounded-model-checking harnesses for the exit-code contract (`CommandError::exit_code`, `gate evaluate` status-to-exit mapping) and the gate decision status (`top_level_status` fails closed on blocking findings, exception blockers and config errors). The failing-status predicate is extracted as `status_fails_gate` with no behavior change.
