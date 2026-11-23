<!-- section: Fixed -->
- `ripr config validate` now checks the suppressions manifest the config selects: a configured `[suppressions] path` that does not exist fails instead of passing (every consumer read it as an empty manifest), and a present manifest that does not parse fails with its line-numbered violations (#15308).
