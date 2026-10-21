<!-- section: Internal / CI -->
- The nightly first-run row receipt now has a gate test that a slower
  `install_seconds` fails against a same-runner baseline, and the `first-run`
  command catalog names `first-run-rows.jsonl`, `install-root`, `cargo-home`,
  and `install_published.stderr.log` (#6871).
