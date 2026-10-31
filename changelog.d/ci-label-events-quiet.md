<!-- section: Changed -->
- The generated `ripr init --ci github` workflow no longer re-runs on unrelated label events in advisory and `visible-only` mode, and a label event can no longer cancel an analysis already running for a push; `labeled` and `unlabeled` each get their own concurrency group (#11753).
