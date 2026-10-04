<!-- section: Fixed -->
- Agent stub: `ripr check` prints the `ripr agent stub` route only when the
  stub resolver yields a stub for that finding, and otherwise prints the
  resolver's refusal. The route ends in `--kind <family>` (the finding's
  probe family), and `--at` tries only seams of that kind, so the stub never
  targets a seam of another kind on or near that line. When the line holds
  two separate seams of that kind (`a > 10 && b > 20`), `--at` refuses and
  lists their seam IDs instead of guessing, and `check` prints no route.
  When the function holds no seam of the finding's kind, `check` prints
  neither a route nor another seam's refusal. `--at` reads the seams of the
  one file from a parse of that file alone, without re-classifying; on
  rust-lang/regex (debug build) warm `ripr check` stays at 5.6-5.7 s and
  `agent stub --at` takes 0.06 s
  ([#5471](https://github.com/EffortlessMetrics/ripr-swarm/issues/5471)).
