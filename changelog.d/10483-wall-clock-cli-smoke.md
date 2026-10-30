<!-- section: Internal / CI -->
- CLI smoke tests for hung-git timeouts, the stalled stdin probe and `pr-summary` no longer assert an elapsed-time ceiling: the hung `git` shim leaves a marker only if its hang runs to the end, a `cargo` shim records any invocation, and the stalled probe's `TimedOut` error already proves the budget fired, so required runs stop failing on runner speed (#10483).
