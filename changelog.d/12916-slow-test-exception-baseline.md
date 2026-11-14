<!-- section: Internal / CI -->
- Refresh the slow-test baseline from Test Analytics run 42511871092 (2026-11-08): every test over 60s is now a named #12916 exception with its verified cost driver, refreshed timings replace stale ones, and two entries at or under 30s are deleted as ratchet candidates. This records the exceptions; it does not reduce the tests (#12916).
