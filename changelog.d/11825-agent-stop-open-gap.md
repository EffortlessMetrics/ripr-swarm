<!-- section: Fixed -->
- `ripr check --format agent` no longer prints `stop=` when a test-side gap
  remains open after triage selects a static-limited finding; the report lists
  the gap and the next `ripr explain` command instead. A run with no remaining
  test-side gap still prints `stop=` (#11825).
