<!-- section: Internal / CI -->
- The `policy_all` xtask test fixtures add a per-process sequence number to
  their temp directory names, so parallel tests can no longer share one
  directory and race two `git init`s on Windows (#10344).
