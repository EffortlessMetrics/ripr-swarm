<!-- section: Internal / CI -->
- `.github/workflows/workflow-lint.yml` retries the `jactionlint` release tarball
  download (`--retry 5 --retry-all-errors`) while preserving the pinned SHA-256
  check and offline lint invocation, with a regression test covering transient
  HTTP 504 recovery, persistent download failure, and checksum mismatch (#13637).
