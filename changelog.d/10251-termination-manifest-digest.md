<!-- section: Internal / CI -->
- `cargo xtask termination-qualification --candidate-manifest` now requires
  `--candidate-manifest-sha256 <accepted-raw-sha256>` and refuses manifest
  bytes that do not match, so an edited or stale #1609 manifest can no longer
  supply the candidate commit. The release transaction runbook writes the
  manifest-proposal input under the controller root the producer reads, the
  source-promotion preflight example uses the runbook's paths, and the
  proposal producer's per-file budget branch has a test (#10251).
