<!-- section: Changed -->
- A nightly and on-demand `first-run-install` workflow now times a cold
  `cargo install ripr --locked`, walks the first-run path, and ingests the
  receipt into the `first_run` scoreboard. It stays ungated until a same-runner
  baseline is committed as `metrics/dx-scoreboard/first-run-baseline.json`; a
  test now shows a slower install failing the gated comparison. Pull requests
  do not run it (#5983).
