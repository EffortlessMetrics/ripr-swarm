<!-- section: Internal / CI -->
- `cargo xtask dx-scoreboard` warm-check sample admission now compares the harness root and the
  producer root in the same plain path form, so Windows warm samples are admitted and measured
  instead of rejected for the `\\?\` verbatim prefix produced by `fs::canonicalize` (#12947).
