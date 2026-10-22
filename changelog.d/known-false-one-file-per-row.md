<!-- section: Internal / CI -->
- Verdict corpus: each known-false row is its own `known-false/<case>.toml` file instead of a shared `[[known_false]]` list, so parallel fix PRs stop conflicting; the gate is unchanged.
