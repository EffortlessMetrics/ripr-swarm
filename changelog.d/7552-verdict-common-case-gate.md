<!-- section: Fixed -->
- `cargo xtask dx-scoreboard` compares verdict rates on unchanged common cases,
  reports corpus additions, removals and changed definitions separately, and
  fails a gate when case identity evidence is missing or invalid. Whole-corpus
  targets and the original Rust baseline values remain unchanged (#7552).
