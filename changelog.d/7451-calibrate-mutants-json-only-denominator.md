<!-- section: Fixed -->
- `ripr calibrate cargo-mutants` now says, in `--help` and on the default
  Markdown Summary, that a `mutants.json`-only import counts unexecuted
  candidates in `matched_total`. Those rows still join as `unknown` /
  `no_runtime_data`; the metric is unchanged (#7451).
