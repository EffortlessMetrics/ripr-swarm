<!-- section: Fixed -->
- CI diagnostics: when the `xtask` fixture-cache contention tests' nested
  child run drifts from its authored goldens, the child assertion now prints
  which surface (`check.json` / `human.txt` / `human-full.txt`) drifted and
  the first differing line, instead of a bare "owned fixture must retain its
  authored goldens". Hosted-runner flakes previously left no comparable
  evidence in the captured child output.
