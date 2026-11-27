<!-- section: Fixed -->
- A `ripr.toml` value of the wrong type now says what belongs there in TOML
  terms and how to write it (`expected a whole number of 1 or more`, then
  ``write a whole number of 1 or more with no quotes, for example `5` ``)
  instead of `expected usize`, `expected a sequence` or an internal
  `struct RawAnalysisConfig` name; `ripr doctor` and the JSON error carry the
  same next step. A switch given a value (`ripr check --quiet=false`) is
  refused as a switch that takes no value, naming the spelling that works,
  instead of as an unknown argument. The `[severity.seams]` accepted-value
  list reads `info, warning, note, or off` (#15898).
