<!-- section: Internal / CI -->
- Verdict corpus: each known-false row is its own `known-false/<case>.toml` file instead of a shared `[[known_false]]` list, and `docs/roadmap/false-verdicts.md` no longer carries per-row tables or counts, so parallel fix PRs stop conflicting; the gate is unchanged (#9430).
