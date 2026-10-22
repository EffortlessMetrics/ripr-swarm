<!-- section: Internal / CI -->
- `verdict-corpus check` now ratchets a per-language `missed_credit_ceiling` in each `spec-coverage.toml`: the count of abstained rows whose tests do catch the change. A rise fails the check; a fall passes with a note to lower the ceiling (#9619).
