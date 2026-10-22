<!-- section: Internal / CI -->
- The termination qualification harness adds a cleanup-failure execution
  control and `--candidate-manifest`, which reads `candidate.sha` from the
  frozen-candidate manifest and refuses a binary built from another commit
  (#4510, #1609, #9480). No product behavior changes.
