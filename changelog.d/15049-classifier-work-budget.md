<!-- section: Internal / CI -->
- The `ripr` test suite now fails when the Rust classifier does more indexed reads per probe on a fixed five-gap diff, so a run-time regression of that kind is caught in the change that causes it instead of by a nightly lane. `docs/PERFORMANCE_GUARDS.md` describes the count, its ceiling and what it does not cover (#15049).
