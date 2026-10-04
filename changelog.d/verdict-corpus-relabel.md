<!-- section: Added -->
- Verdict corpus: `cargo xtask verdict-corpus relabel` replays a case's
  mutants against its own `cargo test` command and fails when an outcome,
  failing test, or derived truth drifts from the label, a mutant does not
  compile or changes nothing, repeated runs disagree, or the toolchain
  differs. `--sample <n> --seed <s>` picks a deterministic subset. Every
  rewrite mutant now carries `mutated_line`, the exact anchor line it
  replays, and `failing_test` must be one test name. A full replay of all
  104 cases on rustc 1.95.0 reproduced every label.
