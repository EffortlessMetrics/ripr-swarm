<!-- section: Fixed -->
- The "Reproduce and refresh" section of `docs/PUBLIC_PROOF.md` now genuinely
  reproduces the page: the listed commands include the `cp` steps that promote
  a local `dx-scoreboard` run into `metrics/dx-scoreboard/baseline.json` and a
  local `first-run` walk into `metrics/public-proof/first-run-current.json`,
  name each harness's output path, add a per-receipt regeneration table, and
  note that a local rerun cannot reproduce the committed numbers without the
  recorded runner class (#8720).
