<!-- section: Internal / CI -->
- `cargo xtask kani` now gives each harness its own timeout (`--harness-timeout-secs`, default 900) and records a solver timeout as `NOT-ESTABLISHED`, so one non-terminating proof no longer blanks every other harness in the run. (#12139)
